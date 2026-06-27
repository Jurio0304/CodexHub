mod ssh;

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    app: &'static str,
    mode: &'static str,
    remote_wrapper_required: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum AuthMethod {
    SshKey,
    Password,
    Agent,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HostStatus {
    Online,
    Offline,
    Unknown,
    Testing,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Host {
    id: String,
    name: String,
    host_alias: String,
    source: String,
    address: String,
    port: u16,
    username: String,
    auth_method: AuthMethod,
    status: HostStatus,
    os: String,
    arch: String,
    shell: String,
    path: Option<String>,
    path_has_local_bin: Option<bool>,
    codex_installed: bool,
    codex_version: String,
    config_exists: Option<bool>,
    skills_exists: Option<bool>,
    skills_count: Option<u16>,
    profile_id: Option<String>,
    skill_pack_ids: Vec<String>,
    tags: Vec<String>,
    last_seen: String,
    latency_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostDraft {
    name: String,
    address: String,
    port: u16,
    username: String,
    auth_method: AuthMethod,
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostPatch {
    name: Option<String>,
    address: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    auth_method: Option<AuthMethod>,
    status: Option<HostStatus>,
    profile_id: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    id: String,
    name: String,
    description: String,
    model: String,
    approval_policy: String,
    sandbox_mode: String,
    updated_at: String,
    host_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillPack {
    id: String,
    name: String,
    version: String,
    description: String,
    source: String,
    skill_count: u16,
    enabled: bool,
    updated_at: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum TaskStatus {
    Queued,
    Running,
    Success,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum TaskLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskLog {
    id: String,
    task_run_id: String,
    level: TaskLogLevel,
    timestamp: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timed_out: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskRun {
    id: String,
    host_id: String,
    host_name: String,
    action: String,
    status: TaskStatus,
    started_at: String,
    ended_at: Option<String>,
    summary: String,
    logs: Vec<TaskLog>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionTest {
    ok: bool,
    latency_ms: Option<u64>,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshCheckResult {
    host_alias: String,
    ok: bool,
    latency_ms: Option<u64>,
    message: String,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshBootstrapResult {
    host_alias: String,
    ok: bool,
    latency_ms: Option<u64>,
    message: String,
    generated_key: bool,
    private_key_path: String,
    public_key_path: String,
    write_result: ssh::SshConfigWriteResult,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshBootstrapProgressEvent {
    request_id: String,
    host_alias: String,
    step: String,
    status: String,
    message: String,
    detail: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    timed_out: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteProbeResult {
    host_alias: String,
    ssh_status: HostStatus,
    os: String,
    arch: String,
    shell: String,
    path: Option<String>,
    path_has_local_bin: bool,
    codex_installed: bool,
    codex_path: Option<String>,
    codex_version: String,
    config_exists: bool,
    skills_exists: bool,
    skills_count: u16,
    task: TaskRun,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum RemoteCodexAction {
    CheckVersion,
    Install,
    Update,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCodexMaintenanceResult {
    host_alias: String,
    ok: bool,
    action: RemoteCodexAction,
    before_version: Option<String>,
    after_version: Option<String>,
    codex_path: Option<String>,
    install_method: Option<String>,
    path_changed: bool,
    shell_config_path: Option<String>,
    backup_path: Option<String>,
    message: String,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCodexProgressEvent {
    request_id: String,
    host_alias: String,
    action: RemoteCodexAction,
    step: String,
    status: String,
    message: String,
    detail: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    timed_out: Option<bool>,
}

struct CodexProgressContext<'a> {
    app: &'a AppHandle,
    request_id: Option<&'a str>,
    host_alias: &'a str,
    action: &'a RemoteCodexAction,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThemeChoice {
    System,
    Light,
    Dark,
}

#[derive(Clone, Serialize, Deserialize)]
enum FontPreset {
    #[serde(
        rename = "english",
        alias = "system",
        alias = "chinese",
        alias = "cross-platform"
    )]
    English,
    #[serde(rename = "zh-cn")]
    ZhCn,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    theme: ThemeChoice,
    font_preset: FontPreset,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            font_preset: FontPreset::English,
        }
    }
}

struct AppState {
    hosts: Mutex<Vec<Host>>,
    profiles: Vec<Profile>,
    skill_packs: Vec<SkillPack>,
    tasks: Mutex<Vec<TaskRun>>,
}

const CODEX_RESOLVER_SCRIPT: &str = r#"best_path=""
best_version=""
best_key=""
seen=""

version_numbers() {
  case "$1" in
    codex*|Codex*|[0-9]*|v[0-9]*) ;;
    *) return 0 ;;
  esac
  printf '%s\n' "$1" | sed -n 's/^[^0-9]*\([0-9][0-9]*\)\.\([0-9][0-9]*\)\.\([0-9][0-9]*\).*/\1 \2 \3/p' | head -n 1
}

package_version_for_candidate() {
  candidate="$1"
  target=$(readlink -f "$candidate" 2>/dev/null || printf '%s\n' "$candidate")
  dir=${target%/*}
  depth=0
  while [ -n "$dir" ] && [ "$dir" != "/" ] && [ "$depth" -lt 6 ]; do
    package_json="$dir/package.json"
    if [ -f "$package_json" ]; then
      package_name=$(sed -n 's/^[[:space:]]*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$package_json" | head -n 1)
      package_version=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$package_json" | head -n 1)
      if [ "$package_name" = "@openai/codex" ] && [ -n "$package_version" ]; then
        printf 'codex-cli %s\n' "$package_version"
        return 0
      fi
    fi
    next_dir=${dir%/*}
    if [ "$next_dir" = "$dir" ]; then
      break
    fi
    dir="$next_dir"
    depth=$((depth + 1))
  done
  return 1
}

probe_candidate() {
  candidate="$1"
  if [ -z "$candidate" ] || [ ! -x "$candidate" ]; then
    return
  fi
  case ":$seen:" in
    *":$candidate:"*) return ;;
  esac
  seen="$seen:$candidate"

  version=$("$candidate" --version </dev/null 2>&1 | head -n 1)
  version_source="command"
  numbers=$(version_numbers "$version")

  if [ -z "$numbers" ]; then
    package_version=$(package_version_for_candidate "$candidate" 2>/dev/null || true)
    package_numbers=$(version_numbers "$package_version")
    if [ -n "$package_numbers" ]; then
      printf 'candidate %s -> %s (package metadata; --version: %s)\n' "$candidate" "$package_version" "${version:-no version output}" >&2
      version="$package_version"
      version_source="package metadata"
      numbers="$package_numbers"
    else
      printf 'candidate %s -> %s\n' "$candidate" "${version:-unversioned}" >&2
      return
    fi
  else
    printf 'candidate %s -> %s\n' "$candidate" "$version" >&2
  fi

  old_numbers_ifs="$IFS"
  IFS=' '
  set -- $numbers
  IFS="$old_numbers_ifs"
  key=$(printf '%06d%06d%06d' "$1" "$2" "$3")
  if [ -z "$best_key" ] || [ "$key" \> "$best_key" ]; then
    best_key="$key"
    best_path="$candidate"
    best_version="$version"
    best_version_source="$version_source"
  fi
}

probe_path_list() {
  old_ifs="$IFS"
  IFS=:
  for dir in $1; do
    probe_candidate "$dir/codex"
  done
  IFS="$old_ifs"
}

probe_path_list "$PATH"

login_shell="${SHELL:-}"
if [ -n "$login_shell" ] && [ -x "$login_shell" ]; then
  login_path=$("$login_shell" -lc 'printf "%s" "$PATH"' 2>/dev/null || true)
  if [ -n "$login_path" ]; then
    probe_path_list "$login_path"
  fi
  login_codex=$("$login_shell" -lc 'command -v codex 2>/dev/null' 2>/dev/null | head -n 1 || true)
  case "$login_codex" in
    /*) probe_candidate "$login_codex" ;;
  esac
fi

for candidate in \
  "$HOME/.local/bin/codex" \
  "$HOME/.npm-global/bin/codex" \
  "$HOME/.npm-packages/bin/codex" \
  "$HOME/.volta/bin/codex" \
  "$HOME/.bun/bin/codex" \
  "$HOME/.cargo/bin/codex" \
  "$HOME/.local/share/pnpm/codex" \
  "$HOME/.asdf/shims/codex" \
  "$HOME/.local/share/mise/shims/codex" \
  "$HOME/node_modules/.bin/codex" \
  "/usr/local/bin/codex" \
  "/usr/bin/codex" \
  "/snap/bin/codex" \
  "/opt/homebrew/bin/codex" \
  "/home/linuxbrew/.linuxbrew/bin/codex"
do
  probe_candidate "$candidate"
done

for candidate in \
  "$HOME/.nvm/versions/node"/*/bin/codex \
  "$HOME/.fnm/node-versions"/*/installation/bin/codex \
  "$HOME/.local/share/fnm/node-versions"/*/installation/bin/codex \
  "$HOME/.local/share/mise/installs/node"/*/bin/codex \
  "$HOME/.local/share/pnpm/global"/*/node_modules/.bin/codex
do
  probe_candidate "$candidate"
done

if [ -z "$best_path" ]; then
  exit 127
fi
printf 'selected %s -> %s (%s)\n' "$best_path" "$best_version" "$best_version_source" >&2"#;

fn codex_path_probe_script() -> String {
    format!("{CODEX_RESOLVER_SCRIPT}\nprintf '%s\\n' \"$best_path\"")
}

fn codex_version_probe_script() -> String {
    format!("{CODEX_RESOLVER_SCRIPT}\nprintf '%s\\n' \"$best_version\"")
}

const CODEX_PATH_REPAIR_SCRIPT: &str = r##"set -u
local_bin="$HOME/.local/bin"
mkdir -p "$local_bin"

case ":$PATH:" in
  *":$local_bin:"*)
    printf 'CODEXHUB_PATH_CHANGED=no\n'
    printf 'CODEXHUB_SHELL_CONFIG_PATH=\n'
    printf 'CODEXHUB_BACKUP_PATH=\n'
    printf 'PATH already contains %s\n' "$local_bin"
    exit 0
    ;;
esac

shell_value=${SHELL:-}
shell_name=${shell_value##*/}
if [ "$shell_name" = "zsh" ]; then
  shell_config="$HOME/.zshrc"
elif [ "$shell_name" = "bash" ]; then
  shell_config="$HOME/.bashrc"
elif [ -f "$HOME/.zshrc" ] && [ ! -f "$HOME/.bashrc" ]; then
  shell_config="$HOME/.zshrc"
else
  shell_config="$HOME/.bashrc"
fi

begin_marker="# >>> CodexHub managed PATH"
end_marker="# <<< CodexHub managed PATH"
path_line='case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac'

if [ -f "$shell_config" ] &&
  grep -F "$begin_marker" "$shell_config" >/dev/null 2>&1 &&
  grep -F "$end_marker" "$shell_config" >/dev/null 2>&1 &&
  grep -F "$path_line" "$shell_config" >/dev/null 2>&1; then
  printf 'CODEXHUB_PATH_CHANGED=no\n'
  printf 'CODEXHUB_SHELL_CONFIG_PATH=%s\n' "$shell_config"
  printf 'CODEXHUB_BACKUP_PATH=\n'
  printf 'CodexHub PATH block is already present in %s\n' "$shell_config"
  exit 0
fi

backup_path=""
if [ -f "$shell_config" ]; then
  backup_path="$shell_config.codexhub.bak.$(date +%Y%m%d%H%M%S)"
  cp -p "$shell_config" "$backup_path"
else
  : >"$shell_config"
fi

tmp_file="$shell_config.codexhub.tmp.$$"
if grep -F "$begin_marker" "$shell_config" >/dev/null 2>&1 &&
  grep -F "$end_marker" "$shell_config" >/dev/null 2>&1; then
  awk -v begin="$begin_marker" -v end="$end_marker" -v line="$path_line" '
    $0 == begin {
      print begin
      print line
      print end
      in_block = 1
      next
    }
    $0 == end && in_block {
      in_block = 0
      next
    }
    !in_block { print }
  ' "$shell_config" >"$tmp_file"
  mv "$tmp_file" "$shell_config"
else
  rm -f "$tmp_file"
  {
    printf '\n%s\n' "$begin_marker"
    printf '%s\n' "$path_line"
    printf '%s\n' "$end_marker"
  } >>"$shell_config"
fi

printf 'CODEXHUB_PATH_CHANGED=yes\n'
printf 'CODEXHUB_SHELL_CONFIG_PATH=%s\n' "$shell_config"
printf 'CODEXHUB_BACKUP_PATH=%s\n' "$backup_path"
printf 'Added CodexHub PATH block to %s\n' "$shell_config"
"##;

const CODEX_INSTALL_SCRIPT: &str = r##"set -u
export CODEX_INSTALL_DIR="$HOME/.local/bin"
export CODEX_HOME="$HOME/.codex"
export CODEX_NON_INTERACTIVE=1
export PATH="$HOME/.local/bin:$PATH"

mkdir -p "$CODEX_INSTALL_DIR" "$CODEX_HOME"
tmp_dir="${TMPDIR:-/tmp}/codexhub-codex-install.$$"
mkdir -p "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM
insecure_tls_fallback=no

is_tls_cert_error() {
  grep -qiE 'certificate|self[- ]signed|local issuer|certificate verify failed|x509' "$1" 2>/dev/null
}

allow_insecure_for_url() {
  case "$1" in
    https://registry.npmmirror.com/* | https://*.npmmirror.com/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

download_file_url() {
  url="$1"
  output="$2"
  allow_insecure="${3:-no}"
  err_file="$tmp_dir/download.err"
  last_status=127

  if [ "$allow_insecure" = "yes" ] && ! allow_insecure_for_url "$url"; then
    printf 'Insecure TLS fallback is limited to npmmirror URLs; refusing disabled verification for %s\n' "$url" >&2
    allow_insecure=no
  fi

  if command -v curl >/dev/null 2>&1; then
    rm -f "$err_file"
    if curl -fsSL "$url" -o "$output" 2>"$err_file"; then
      return 0
    fi
    last_status=$?
    cat "$err_file" >&2
    if [ "$allow_insecure" = "yes" ] && is_tls_cert_error "$err_file"; then
      printf 'TLS certificate verification failed for %s; retrying npmmirror download with certificate checks disabled.\n' "$url" >&2
      rm -f "$err_file"
      if curl -k -fsSL "$url" -o "$output" 2>"$err_file"; then
        insecure_tls_fallback=yes
        return 0
      fi
      last_status=$?
      cat "$err_file" >&2
    fi
  fi
  if command -v wget >/dev/null 2>&1; then
    rm -f "$err_file"
    if wget -qO "$output" "$url" 2>"$err_file"; then
      return 0
    fi
    last_status=$?
    cat "$err_file" >&2
    if [ "$allow_insecure" = "yes" ] && is_tls_cert_error "$err_file"; then
      printf 'TLS certificate verification failed for %s; retrying npmmirror download with certificate checks disabled.\n' "$url" >&2
      rm -f "$err_file"
      if wget --no-check-certificate -qO "$output" "$url" 2>"$err_file"; then
        insecure_tls_fallback=yes
        return 0
      fi
      last_status=$?
      cat "$err_file" >&2
    fi
  fi
  return "$last_status"
}

looks_like_captive_portal() {
  grep -qiE '<html|authentication is required|net2\.zju\.edu\.cn|captive|portal|login' "$1" 2>/dev/null
}

extract_npmmirror_metadata() {
  metadata_file="$1"
  package_platform="$2"

  if looks_like_captive_portal "$metadata_file"; then
    printf 'npmmirror metadata response was HTML instead of JSON; the remote network may require captive portal authentication before downloads can work.\n' >&2
    return 65
  fi

  python3 - "$metadata_file" "$package_platform" <<'PY'
import json
import sys

metadata_file = sys.argv[1]
package_platform = sys.argv[2]

try:
    with open(metadata_file, encoding="utf-8") as handle:
        data = json.load(handle)
except json.JSONDecodeError as exc:
    print(
        "npmmirror metadata response was not JSON; the remote network may require captive portal authentication before downloads can work: "
        + str(exc),
        file=sys.stderr,
    )
    sys.exit(65)
except OSError as exc:
    print("Could not read npmmirror metadata response: " + str(exc), file=sys.stderr)
    sys.exit(65)

latest = data.get("dist-tags", {}).get("latest")
if not isinstance(latest, str) or not latest:
    print("npmmirror metadata did not include dist-tags.latest for @openai/codex.", file=sys.stderr)
    sys.exit(66)

package_key = latest + "-" + package_platform
package = data.get("versions", {}).get(package_key)
if not isinstance(package, dict):
    print("npmmirror metadata did not include package version " + package_key + ".", file=sys.stderr)
    sys.exit(66)

tarball = package.get("dist", {}).get("tarball")
if not isinstance(tarball, str) or not tarball.startswith("https://registry.npmmirror.com/"):
    print("npmmirror metadata returned an unexpected tarball URL for " + package_key + ".", file=sys.stderr)
    sys.exit(66)

print("CODEXHUB_NATIVE_VERSION=" + latest)
print("CODEXHUB_NATIVE_TARBALL=" + tarball)
PY
}

official_status=127
if command -v curl >/dev/null 2>&1; then
  if curl -fsSL --connect-timeout 15 --max-time 45 "https://chatgpt.com/codex/install.sh" -o "$tmp_dir/install.sh" 2>"$tmp_dir/official.err"; then
    if command -v timeout >/dev/null 2>&1; then
      timeout 75 sh "$tmp_dir/install.sh"
    else
      sh "$tmp_dir/install.sh"
    fi
    official_status=$?
  else
    official_status=$?
    cat "$tmp_dir/official.err" >&2
    if is_tls_cert_error "$tmp_dir/official.err"; then
      printf 'Official Codex installer TLS verification failed; falling back to npmmirror. CodexHub will not disable TLS verification for the official installer.\n' >&2
    fi
  fi
elif command -v wget >/dev/null 2>&1; then
  if wget --timeout=15 --tries=1 -qO "$tmp_dir/install.sh" "https://chatgpt.com/codex/install.sh" 2>"$tmp_dir/official.err"; then
    if command -v timeout >/dev/null 2>&1; then
      timeout 75 sh "$tmp_dir/install.sh"
    else
      sh "$tmp_dir/install.sh"
    fi
    official_status=$?
  else
    official_status=$?
    cat "$tmp_dir/official.err" >&2
    if is_tls_cert_error "$tmp_dir/official.err"; then
      printf 'Official Codex installer TLS verification failed; falling back to npmmirror. CodexHub will not disable TLS verification for the official installer.\n' >&2
    fi
  fi
else
  printf 'curl or wget is not available for the official Codex installer.\n' >&2
fi

if [ "$official_status" -eq 0 ]; then
  printf 'CODEXHUB_INSTALL_METHOD=official\n'
  exit 0
fi

printf 'Official Codex installer failed with status %s; trying npmmirror native package fallback.\n' "$official_status" >&2
native_status=127
if command -v python3 >/dev/null 2>&1; then
  arch=$(uname -m)
  case "$arch" in
    x86_64 | amd64)
      platform="linux-x64"
      target="x86_64-unknown-linux-musl"
      ;;
    aarch64 | arm64)
      platform="linux-arm64"
      target="aarch64-unknown-linux-musl"
      ;;
    *)
      platform=""
      target=""
      ;;
  esac

  version=""
  tarball=""
  if [ -n "$platform" ] && download_file_url "https://registry.npmmirror.com/@openai/codex" "$tmp_dir/codex-metadata.json" yes; then
    metadata_out="$tmp_dir/codex-metadata.out"
    if extract_npmmirror_metadata "$tmp_dir/codex-metadata.json" "$platform" >"$metadata_out"; then
      version=$(sed -n 's/^CODEXHUB_NATIVE_VERSION=//p' "$metadata_out" | head -n 1)
      tarball=$(sed -n 's/^CODEXHUB_NATIVE_TARBALL=//p' "$metadata_out" | head -n 1)
    else
      native_status=$?
    fi
    if [ -n "$version" ] && [ -n "$tarball" ] && download_file_url "$tarball" "$tmp_dir/codex-platform.tgz" yes; then
      extract_dir="$tmp_dir/native-extract"
      release_dir="$CODEX_HOME/packages/standalone/releases/$version"
      stage_dir="$release_dir.tmp.$$"
      rm -rf "$extract_dir" "$stage_dir"
      mkdir -p "$extract_dir" "$stage_dir" "$CODEX_HOME/packages/standalone/releases"
      if ! tar -tzf "$tmp_dir/codex-platform.tgz" >/dev/null 2>&1; then
        printf 'npmmirror native package download was not a readable gzip tarball; the remote network may be returning an HTML login page instead of the package.\n' >&2
        native_status=65
      elif tar -tzf "$tmp_dir/codex-platform.tgz" | grep -Eq '(^|/)\.\.(/|$)|^/'; then
        printf 'npmmirror native package archive contains unsafe paths.\n' >&2
        native_status=66
      elif tar -xzf "$tmp_dir/codex-platform.tgz" -C "$extract_dir"; then
        vendor_dir="$extract_dir/package/vendor/$target"
        if [ -x "$vendor_dir/bin/codex" ]; then
          cp -R "$vendor_dir/." "$stage_dir/"
          chmod 0755 "$stage_dir/bin/codex"
          [ -f "$stage_dir/codex-path/rg" ] && chmod 0755 "$stage_dir/codex-path/rg"
          [ -f "$stage_dir/codex-resources/bwrap" ] && chmod 0755 "$stage_dir/codex-resources/bwrap"
          rm -rf "$release_dir"
          mv "$stage_dir" "$release_dir"
          ln -sfn "$release_dir" "$CODEX_HOME/packages/standalone/current"
          ln -sfn "$release_dir/bin/codex" "$CODEX_INSTALL_DIR/codex"
          native_status=0
        else
          printf 'npmmirror native package did not contain vendor/%s/bin/codex.\n' "$target" >&2
          native_status=66
        fi
      else
        native_status=$?
      fi
      rm -rf "$stage_dir"
    fi
  elif [ -z "$platform" ]; then
    printf 'npmmirror native package fallback does not support remote architecture %s.\n' "$arch" >&2
  fi
fi

if [ "$native_status" -eq 0 ]; then
  if [ "$insecure_tls_fallback" = "yes" ]; then
    printf 'CODEXHUB_INSTALL_METHOD=npm-mirror-native-insecure-tls\n'
  else
    printf 'CODEXHUB_INSTALL_METHOD=npm-mirror-native\n'
  fi
  exit 0
fi

printf 'npmmirror native package fallback failed with status %s; trying npm command fallback.\n' "$native_status" >&2
if ! command -v npm >/dev/null 2>&1; then
  printf 'npm is not available for the npmmirror fallback.\n' >&2
  printf 'CODEXHUB_INSTALL_METHOD=failed\n'
  exit 127
fi

npm install -g @openai/codex --prefix "$HOME/.local" --registry=https://registry.npmmirror.com
npm_status=$?
if [ "$npm_status" -eq 0 ]; then
  printf 'CODEXHUB_INSTALL_METHOD=npm-mirror\n'
  exit 0
fi

printf 'CODEXHUB_INSTALL_METHOD=failed\n'
exit "$npm_status"
"##;

const CODEX_NATIVE_PLATFORM_SCRIPT: &str = r#"set -u
arch=$(uname -m)
case "$arch" in
  x86_64 | amd64)
    platform="linux-x64"
    target="x86_64-unknown-linux-musl"
    ;;
  aarch64 | arm64)
    platform="linux-arm64"
    target="aarch64-unknown-linux-musl"
    ;;
  *)
    printf 'CodexHub local upload fallback does not support remote architecture %s.\n' "$arch" >&2
    exit 2
    ;;
esac

printf 'CODEXHUB_NATIVE_PLATFORM=%s\n' "$platform"
printf 'CODEXHUB_NATIVE_TARGET=%s\n' "$target"
"#;

struct LocalCodexNativePackage {
    version: String,
    target: String,
    tarball_path: PathBuf,
    temp_dir: PathBuf,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            hosts: Mutex::new(mock_hosts()),
            profiles: mock_profiles(),
            skill_packs: mock_skill_packs(),
            tasks: Mutex::new(mock_tasks()),
        }
    }
}

#[tauri::command]
fn app_health() -> Health {
    Health {
        app: "CodexHub",
        mode: "tauri",
        remote_wrapper_required: false,
    }
}

async fn run_blocking_command<T, F>(label: &'static str, command: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(command)
        .await
        .map_err(|error| format!("{label} worker failed: {error}"))
}

#[tauri::command]
fn get_settings(app: AppHandle) -> AppSettings {
    read_settings(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    write_settings(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
fn get_ssh_status() -> Result<ssh::SshStatus, String> {
    ssh::get_ssh_status()
}

#[tauri::command]
fn generate_ed25519_key() -> Result<ssh::SshKeyGenerationResult, String> {
    ssh::generate_ed25519_key()
}

#[tauri::command]
fn list_ssh_config_hosts() -> Result<Vec<ssh::SshConfigHost>, String> {
    ssh::list_ssh_config_hosts()
}

#[tauri::command]
fn upsert_ssh_config_host(draft: ssh::SshHostDraft) -> Result<ssh::SshConfigWriteResult, String> {
    ssh::upsert_ssh_config_host(draft)
}

#[tauri::command]
fn delete_ssh_config_host(alias: String) -> Result<ssh::SshConfigWriteResult, String> {
    ssh::delete_ssh_config_host(alias)
}

#[tauri::command]
fn list_hosts(state: State<'_, AppState>) -> Vec<Host> {
    let _ = merge_discovered_hosts(&state);
    state.hosts.lock().expect("hosts mutex poisoned").clone()
}

#[tauri::command]
fn refresh_discovered_hosts(state: State<'_, AppState>) -> Result<Vec<Host>, String> {
    merge_discovered_hosts(&state)?;
    Ok(state.hosts.lock().expect("hosts mutex poisoned").clone())
}

#[tauri::command]
fn add_host(state: State<'_, AppState>, draft: HostDraft) -> Host {
    let host = Host {
        id: format!("host-{}", timestamp_millis()),
        name: draft.name,
        host_alias: draft.address.clone(),
        source: "manual".into(),
        address: draft.address,
        port: draft.port,
        username: draft.username,
        auth_method: draft.auth_method,
        status: HostStatus::Unknown,
        os: "Unknown".into(),
        arch: "Unknown".into(),
        shell: "Unknown".into(),
        path: None,
        path_has_local_bin: None,
        codex_installed: false,
        codex_version: "pending".into(),
        config_exists: None,
        skills_exists: None,
        skills_count: None,
        profile_id: None,
        skill_pack_ids: Vec::new(),
        tags: draft.tags,
        last_seen: "just added".into(),
        latency_ms: None,
    };

    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .insert(0, host.clone());
    host
}

#[tauri::command]
fn update_host(state: State<'_, AppState>, id: String, patch: HostPatch) -> Host {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");

    if let Some(host) = hosts.iter_mut().find(|host| host.id == id) {
        if let Some(name) = patch.name {
            host.name = name;
        }
        if let Some(address) = patch.address {
            host.address = address;
        }
        if let Some(port) = patch.port {
            host.port = port;
        }
        if let Some(username) = patch.username {
            host.username = username;
        }
        if let Some(auth_method) = patch.auth_method {
            host.auth_method = auth_method;
        }
        if let Some(status) = patch.status {
            host.status = status;
        }
        if let Some(profile_id) = patch.profile_id {
            host.profile_id = Some(profile_id);
        }
        if let Some(tags) = patch.tags {
            host.tags = tags;
        }
        return host.clone();
    }

    let host = Host {
        id,
        name: patch.name.unwrap_or_else(|| "Mock Host".into()),
        host_alias: patch.address.clone().unwrap_or_else(|| "127.0.0.1".into()),
        source: "manual".into(),
        address: patch.address.unwrap_or_else(|| "127.0.0.1".into()),
        port: patch.port.unwrap_or(22),
        username: patch.username.unwrap_or_else(|| "codex".into()),
        auth_method: patch.auth_method.unwrap_or(AuthMethod::SshKey),
        status: patch.status.unwrap_or(HostStatus::Unknown),
        os: "Unknown".into(),
        arch: "Unknown".into(),
        shell: "Unknown".into(),
        path: None,
        path_has_local_bin: None,
        codex_installed: false,
        codex_version: "pending".into(),
        config_exists: None,
        skills_exists: None,
        skills_count: None,
        profile_id: patch.profile_id,
        skill_pack_ids: Vec::new(),
        tags: patch.tags.unwrap_or_default(),
        last_seen: "just added".into(),
        latency_ms: None,
    };
    hosts.insert(0, host.clone());
    host
}

#[tauri::command]
fn delete_host(state: State<'_, AppState>, id: String) -> bool {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    let before = hosts.len();
    hosts.retain(|host| host.id != id);
    hosts.len() != before
}

#[tauri::command]
fn test_ssh_connection(state: State<'_, AppState>, id: String) -> ConnectionTest {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");

    if let Some(host) = hosts.iter_mut().find(|host| host.id == id) {
        let ok = host.id != "linux-runner";
        host.status = if ok {
            HostStatus::Online
        } else {
            HostStatus::Offline
        };
        host.latency_ms = if ok { Some(24) } else { None };
        if ok {
            host.last_seen = "just now".into();
        }

        return ConnectionTest {
            ok,
            latency_ms: host.latency_ms,
            message: if ok {
                format!("Mock SSH handshake to {} completed.", host.name)
            } else {
                format!("Mock SSH handshake to {} timed out.", host.name)
            },
        };
    }

    ConnectionTest {
        ok: false,
        latency_ms: None,
        message: "Host not found.".into(),
    }
}

#[tauri::command]
async fn ssh_check(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<SshCheckResult, String> {
    run_blocking_command("ssh_check", move || {
        let state = app.state::<AppState>();
        run_ssh_check(&state, host_alias, timeout_ms)
    })
    .await
}

#[tauri::command]
fn bootstrap_ssh_host(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ssh::SshHostDraft,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<SshBootstrapResult, String> {
    run_ssh_bootstrap(&app, &state, draft, password, timeout_ms, request_id)
}

#[tauri::command]
fn bootstrap_existing_ssh_host(
    state: State<'_, AppState>,
    host_alias: String,
    password: String,
    timeout_ms: Option<u64>,
) -> Result<SshBootstrapResult, String> {
    run_existing_ssh_bootstrap(&state, host_alias, password, timeout_ms)
}

#[tauri::command]
async fn remote_probe_codex(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteProbeResult, String> {
    run_blocking_command("remote_probe_codex", move || {
        let state = app.state::<AppState>();
        run_remote_probe(&state, host_alias, timeout_ms)
    })
    .await
}

#[tauri::command]
async fn remote_manage_codex(
    app: AppHandle,
    host_alias: String,
    action: RemoteCodexAction,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteCodexMaintenanceResult, String> {
    run_blocking_command("remote_manage_codex", move || {
        let state = app.state::<AppState>();
        run_remote_manage_codex(&app, &state, host_alias, action, timeout_ms, request_id)
    })
    .await
}

#[tauri::command]
fn list_profiles(state: State<'_, AppState>) -> Vec<Profile> {
    state.profiles.clone()
}

#[tauri::command]
fn list_skill_packs(state: State<'_, AppState>) -> Vec<SkillPack> {
    state.skill_packs.clone()
}

#[tauri::command]
fn apply_profile(state: State<'_, AppState>, profile_id: String, host_ids: Vec<String>) -> TaskRun {
    let profile_name = state
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| profile_id.clone());

    let hosts = state.hosts.lock().expect("hosts mutex poisoned");
    let first_host_id = host_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "no-host".into());
    let host_name = hosts
        .iter()
        .find(|host| host.id == first_host_id)
        .map(|host| host.name.clone())
        .unwrap_or_else(|| "No host selected".into());
    drop(hosts);

    let task_id = format!("task-{}", timestamp_millis());
    let task = TaskRun {
        id: task_id.clone(),
        host_id: first_host_id,
        host_name: host_name.clone(),
        action: "Apply profile".into(),
        status: TaskStatus::Success,
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: format!(
            "{} applied to {} through mock backend.",
            profile_name, host_name
        ),
        logs: vec![
            basic_log(
                &task_id,
                1,
                TaskLogLevel::Info,
                "Reserved apply_profile command accepted host selection.",
            ),
            basic_log(
                &task_id,
                2,
                TaskLogLevel::Info,
                "No remote files were changed; this is mock data only.",
            ),
        ],
    };

    state
        .tasks
        .lock()
        .expect("tasks mutex poisoned")
        .insert(0, task.clone());
    task
}

#[tauri::command]
fn list_tasks(state: State<'_, AppState>) -> Vec<TaskRun> {
    state.tasks.lock().expect("tasks mutex poisoned").clone()
}

fn merge_discovered_hosts(state: &AppState) -> Result<(), String> {
    let discovered_hosts = ssh::list_ssh_config_hosts()?;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    for discovered in discovered_hosts {
        merge_discovered_host(&mut hosts, discovered);
    }
    Ok(())
}

fn merge_discovered_host(hosts: &mut Vec<Host>, discovered: ssh::SshConfigHost) {
    if let Some(existing) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(&discovered.alias))
    {
        existing.source = discovered.source.clone();
        existing.address = host_address(&discovered);
        existing.port = discovered.port;
        if !discovered.user.is_empty() {
            existing.username = discovered.user.clone();
        }
        existing.auth_method = if discovered.identity_file.is_empty() {
            AuthMethod::Agent
        } else {
            AuthMethod::SshKey
        };
        ensure_tag(&mut existing.tags, "ssh-config");
        ensure_tag(&mut existing.tags, &discovered.source);
        return;
    }

    let mut tags = vec!["ssh-config".into(), discovered.source.clone()];
    if discovered.managed {
        tags.push("codexhub-managed".into());
    }

    hosts.insert(
        0,
        Host {
            id: discovered_host_id(&discovered.alias),
            name: discovered.alias.clone(),
            host_alias: discovered.alias.clone(),
            source: discovered.source.clone(),
            address: host_address(&discovered),
            port: discovered.port,
            username: discovered.user.clone(),
            auth_method: if discovered.identity_file.is_empty() {
                AuthMethod::Agent
            } else {
                AuthMethod::SshKey
            },
            status: HostStatus::Unknown,
            os: "Unknown".into(),
            arch: "Unknown".into(),
            shell: "Unknown".into(),
            path: None,
            path_has_local_bin: None,
            codex_installed: false,
            codex_version: "pending".into(),
            config_exists: None,
            skills_exists: None,
            skills_count: None,
            profile_id: None,
            skill_pack_ids: Vec::new(),
            tags,
            last_seen: "not tested".into(),
            latency_ms: None,
        },
    );
}

fn host_address(host: &ssh::SshConfigHost) -> String {
    if host.host_name.is_empty() {
        host.alias.clone()
    } else {
        host.host_name.clone()
    }
}

fn discovered_host_id(alias: &str) -> String {
    let safe = alias
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    format!("ssh-{safe}")
}

fn ensure_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|item| item == tag) {
        tags.push(tag.to_string());
    }
}

fn run_ssh_check(state: &AppState, host_alias: String, timeout_ms: Option<u64>) -> SshCheckResult {
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let task_id = format!("task-ssh-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let output = match alias_result {
        Ok(valid_alias) => ssh::run_ssh_echo_ok(&valid_alias, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {valid_alias} echo ok"),
                format!("Could not start ssh: {error}"),
            )
        }),
        Err(error) => failed_command_output("ssh <invalid-alias> echo ok".into(), error),
    };

    let ok = output.success() && output.stdout.trim() == "ok";
    let message = ssh_check_message(&alias, &output, ok, timeout);
    let task = TaskRun {
        id: task_id.clone(),
        host_id: host_id.clone(),
        host_name: host_name.clone(),
        action: "Test SSH connection".into(),
        status: if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: message.clone(),
        logs: vec![command_log(
            &task_id,
            1,
            if ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            &message,
            &output,
        )],
    };

    update_host_check(state, &alias, ok, output.duration_ms);
    record_task(state, task.clone());

    SshCheckResult {
        host_alias: alias,
        ok,
        latency_ms: if ok { Some(output.duration_ms) } else { None },
        message,
        task,
    }
}

fn run_ssh_bootstrap(
    app: &AppHandle,
    state: &AppState,
    draft: ssh::SshHostDraft,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<SshBootstrapResult, String> {
    let host = ssh::prepare_ssh_config_host(draft)?;
    let write_result = pending_ssh_config_write_result(
        &host,
        "SSH config will be written after password login and key setup succeed.",
    );
    run_bootstrap_for_config_host(
        Some(app),
        state,
        host,
        write_result,
        password,
        timeout_ms,
        request_id,
        true,
    )
}

fn run_existing_ssh_bootstrap(
    state: &AppState,
    host_alias: String,
    password: String,
    timeout_ms: Option<u64>,
) -> Result<SshBootstrapResult, String> {
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let mut host = ssh::list_ssh_config_hosts()?
        .into_iter()
        .find(|host| host.alias.eq_ignore_ascii_case(&alias))
        .ok_or_else(|| format!("Host {alias} was not found in SSH config."))?;
    if host.identity_file.trim().is_empty() {
        host.identity_file = ssh::get_ssh_status()?.preferred_identity_file;
    }
    let write_result = ssh::SshConfigWriteResult {
        changed: false,
        action: "unchanged".into(),
        config_path: ssh::get_ssh_status()?.config_path,
        backup_path: None,
        host: Some(host.clone()),
        message: format!("Using existing SSH config Host {alias}; config was not modified."),
    };
    run_bootstrap_for_config_host(
        None,
        state,
        host,
        write_result,
        password,
        timeout_ms,
        None,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_bootstrap_for_config_host(
    app: Option<&AppHandle>,
    state: &AppState,
    host: ssh::SshConfigHost,
    mut write_result: ssh::SshConfigWriteResult,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
    write_config_before_verify: bool,
) -> Result<SshBootstrapResult, String> {
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let alias = host.alias.clone();
    let request_id =
        request_id.unwrap_or_else(|| format!("bootstrap-{alias}-{}", timestamp_millis()));
    let task_id = format!("task-bootstrap-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let mut logs = Vec::new();
    let mut private_key_path = host.identity_file.clone();
    let mut public_key_path = String::new();

    let key_pair = match ssh::ensure_identity_keypair(&host.identity_file) {
        Ok(key_pair) => {
            private_key_path = key_pair.private_path.display().to_string();
            public_key_path = key_pair.public_path.display().to_string();
            logs.push(basic_log(
                &task_id,
                logs.len() + 1,
                TaskLogLevel::Info,
                if key_pair.generated {
                    "Generated a local Ed25519 identity key for this host."
                } else {
                    "Using an existing local identity key for this host."
                },
            ));
            key_pair
        }
        Err(error) => {
            let output = failed_command_output("ensure local identity key".into(), error);
            let message = format!(
                "SSH bootstrap for {alias} failed: {}",
                command_detail(&output)
            );
            logs.push(command_log(
                &task_id,
                logs.len() + 1,
                TaskLogLevel::Error,
                &message,
                &output,
            ));
            let task = bootstrap_task(
                &task_id,
                &host_id,
                &host_name,
                TaskStatus::Failed,
                &message,
                logs,
            );
            record_task(state, task.clone());
            return Ok(SshBootstrapResult {
                host_alias: alias,
                ok: false,
                latency_ms: None,
                message,
                generated_key: false,
                private_key_path,
                public_key_path,
                write_result,
                task,
            });
        }
    };

    let bootstrap_draft = ssh::SshHostDraft {
        alias: host.alias.clone(),
        host_name: host.host_name.clone(),
        port: host.port,
        user: host.user.clone(),
        identity_file: host.identity_file.clone(),
    };

    let remote_outputs = ssh::run_password_bootstrap_steps(
        &bootstrap_draft,
        &password,
        &key_pair.public_key,
        timeout,
        |step| {
            if let Some(handle) = app {
                emit_bootstrap_progress(
                    handle,
                    &request_id,
                    &alias,
                    bootstrap_progress_step(step),
                    "running",
                    bootstrap_step_running_message(step),
                    None,
                );
            }
        },
        |step, output| {
            if let Some(handle) = app {
                let ok = output.success();
                let message = bootstrap_step_message(step, &alias, output, ok);
                emit_bootstrap_progress(
                    handle,
                    &request_id,
                    &alias,
                    bootstrap_progress_step(step),
                    if ok { "success" } else { "failed" },
                    &message,
                    Some(output),
                );
            }
        },
    );

    for (step, output) in &remote_outputs {
        let ok = output.success();
        let message = bootstrap_step_message(*step, &alias, output, ok);
        logs.push(command_log(
            &task_id,
            logs.len() + 1,
            if ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            &message,
            output,
        ));
        if !ok {
            let message = format!(
                "SSH bootstrap for {alias} failed: {}",
                command_detail(output)
            );
            let task = bootstrap_task(
                &task_id,
                &host_id,
                &host_name,
                TaskStatus::Failed,
                &message,
                logs,
            );
            update_host_check(state, &alias, false, output.duration_ms);
            record_task(state, task.clone());
            return Ok(SshBootstrapResult {
                host_alias: alias,
                ok: false,
                latency_ms: None,
                message,
                generated_key: key_pair.generated,
                private_key_path,
                public_key_path,
                write_result,
                task,
            });
        }
    }

    if remote_outputs.is_empty() {
        let output = failed_command_output(
            "ssh password login".into(),
            "Password bootstrap did not start.".into(),
        );
        let message = format!(
            "SSH bootstrap for {alias} failed: {}",
            command_detail(&output)
        );
        logs.push(command_log(
            &task_id,
            logs.len() + 1,
            TaskLogLevel::Error,
            &message,
            &output,
        ));
        let task = bootstrap_task(
            &task_id,
            &host_id,
            &host_name,
            TaskStatus::Failed,
            &message,
            logs,
        );
        record_task(state, task.clone());
        return Ok(SshBootstrapResult {
            host_alias: alias,
            ok: false,
            latency_ms: None,
            message,
            generated_key: key_pair.generated,
            private_key_path,
            public_key_path,
            write_result,
            task,
        });
    }

    if let Some(handle) = app {
        emit_bootstrap_progress(
            handle,
            &request_id,
            &alias,
            "verify_alias_login",
            "running",
            "Writing CodexHub-managed SSH config and testing alias login...",
            None,
        );
    }

    if write_config_before_verify {
        let draft_for_write = ssh::SshHostDraft {
            alias: host.alias.clone(),
            host_name: host.host_name.clone(),
            port: host.port,
            user: host.user.clone(),
            identity_file: host.identity_file.clone(),
        };
        match ssh::upsert_ssh_config_host(draft_for_write) {
            Ok(result) => {
                write_result = result;
                let message = if let Some(backup) = &write_result.backup_path {
                    format!("SSH config saved; backup: {backup}")
                } else {
                    write_result.message.clone()
                };
                logs.push(basic_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Info,
                    &message,
                ));
            }
            Err(error) => {
                let output =
                    failed_command_output("write CodexHub-managed SSH config".into(), error);
                let message = format!(
                    "Could not write SSH config for {alias}: {}",
                    command_detail(&output)
                );
                logs.push(command_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Error,
                    &message,
                    &output,
                ));
                if let Some(handle) = app {
                    emit_bootstrap_progress(
                        handle,
                        &request_id,
                        &alias,
                        "verify_alias_login",
                        "failed",
                        &message,
                        Some(&output),
                    );
                }
                let task = bootstrap_task(
                    &task_id,
                    &host_id,
                    &host_name,
                    TaskStatus::Failed,
                    &message,
                    logs,
                );
                record_task(state, task.clone());
                return Ok(SshBootstrapResult {
                    host_alias: alias,
                    ok: false,
                    latency_ms: None,
                    message,
                    generated_key: key_pair.generated,
                    private_key_path,
                    public_key_path,
                    write_result,
                    task,
                });
            }
        }
    } else {
        logs.push(basic_log(
            &task_id,
            logs.len() + 1,
            TaskLogLevel::Info,
            "Using existing SSH config; unmanaged user blocks were not modified.",
        ));
    }

    let check_output = ssh::run_ssh_echo_ok(&alias, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} echo ok"),
            format!("Could not start ssh: {error}"),
        )
    });
    let ok = check_output.success() && check_output.stdout.trim() == "ok";
    let mut message = if ok {
        format!("SSH bootstrap for {alias} completed; key login returned ok.")
    } else {
        format!(
            "SSH bootstrap installed the key, but key-login test failed: {}",
            command_detail(&check_output)
        )
    };
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &message,
        &check_output,
    ));

    if !ok && write_config_before_verify && write_result.action == "added" {
        match ssh::delete_ssh_config_host(alias.clone()) {
            Ok(mut rollback) => {
                rollback.action = "rolled_back".into();
                rollback.message = format!(
                    "Rolled back CodexHub-managed Host {alias} after failed key-login verification."
                );
                let rollback_message = if let Some(backup) = &rollback.backup_path {
                    format!("{} Backup: {backup}", rollback.message)
                } else {
                    rollback.message.clone()
                };
                logs.push(basic_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Warn,
                    &rollback_message,
                ));
                message = format!("{message} {rollback_message}");
                write_result = rollback;
            }
            Err(error) => {
                let output =
                    failed_command_output("rollback CodexHub-managed SSH config".into(), error);
                let rollback_message = format!(
                    "Could not roll back CodexHub-managed Host {alias}: {}",
                    command_detail(&output)
                );
                logs.push(command_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Error,
                    &rollback_message,
                    &output,
                ));
                message = format!("{message} {rollback_message}");
            }
        }
    }

    if let Some(handle) = app {
        emit_bootstrap_progress(
            handle,
            &request_id,
            &alias,
            "verify_alias_login",
            if ok { "success" } else { "failed" },
            &message,
            Some(&check_output),
        );
    }

    let task = bootstrap_task(
        &task_id,
        &host_id,
        &host_name,
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &message,
        logs,
    );

    if ok {
        let _ = merge_discovered_hosts(state);
    }
    update_host_check(state, &alias, ok, check_output.duration_ms);
    record_task(state, task.clone());

    Ok(SshBootstrapResult {
        host_alias: alias,
        ok,
        latency_ms: if ok {
            Some(check_output.duration_ms)
        } else {
            None
        },
        message,
        generated_key: key_pair.generated,
        private_key_path,
        public_key_path,
        write_result,
        task,
    })
}
fn pending_ssh_config_write_result(
    host: &ssh::SshConfigHost,
    message: &str,
) -> ssh::SshConfigWriteResult {
    ssh::SshConfigWriteResult {
        changed: false,
        action: "pending".into(),
        config_path: ssh::get_ssh_status()
            .map(|status| status.config_path)
            .unwrap_or_else(|_| "%USERPROFILE%\\.ssh\\config".into()),
        backup_path: None,
        host: Some(host.clone()),
        message: message.into(),
    }
}

fn emit_bootstrap_progress(
    app: &AppHandle,
    request_id: &str,
    host_alias: &str,
    step: &str,
    status: &str,
    message: &str,
    output: Option<&ssh::SshCommandOutput>,
) {
    let payload = SshBootstrapProgressEvent {
        request_id: request_id.to_string(),
        host_alias: host_alias.to_string(),
        step: step.to_string(),
        status: status.to_string(),
        message: message.to_string(),
        detail: output.map(command_detail),
        stdout: output.map(|item| item.stdout.clone()),
        stderr: output.map(|item| item.stderr.clone()),
        exit_code: output.and_then(|item| item.exit_code),
        duration_ms: output.map(|item| item.duration_ms),
        timed_out: output.map(|item| item.timed_out),
    };
    let _ = app.emit("ssh-bootstrap-progress", payload);
}

fn bootstrap_progress_step(step: ssh::PasswordBootstrapStep) -> &'static str {
    match step {
        ssh::PasswordBootstrapStep::PasswordLogin => "password_login",
        ssh::PasswordBootstrapStep::InstallPublicKey => "install_public_key",
        ssh::PasswordBootstrapStep::SetPermissions => "set_permissions",
    }
}

fn bootstrap_step_running_message(step: ssh::PasswordBootstrapStep) -> &'static str {
    match step {
        ssh::PasswordBootstrapStep::PasswordLogin => {
            "Logging in to the remote host with the one-time password..."
        }
        ssh::PasswordBootstrapStep::InstallPublicKey => {
            "Installing the local public key into remote authorized_keys..."
        }
        ssh::PasswordBootstrapStep::SetPermissions => {
            "Setting remote ~/.ssh and authorized_keys permissions..."
        }
    }
}

fn bootstrap_step_message(
    step: ssh::PasswordBootstrapStep,
    alias: &str,
    output: &ssh::SshCommandOutput,
    ok: bool,
) -> String {
    match (step, ok) {
        (ssh::PasswordBootstrapStep::PasswordLogin, true) => {
            format!("Password login to {alias} succeeded.")
        }
        (ssh::PasswordBootstrapStep::PasswordLogin, false) => {
            format!(
                "Password login to {alias} failed: {}",
                command_detail(output)
            )
        }
        (ssh::PasswordBootstrapStep::InstallPublicKey, true) => {
            "Installed or confirmed the local public key in remote authorized_keys.".into()
        }
        (ssh::PasswordBootstrapStep::InstallPublicKey, false) => {
            format!(
                "Could not install public key on {alias}: {}",
                command_detail(output)
            )
        }
        (ssh::PasswordBootstrapStep::SetPermissions, true) => {
            "Remote ~/.ssh permissions were set.".into()
        }
        (ssh::PasswordBootstrapStep::SetPermissions, false) => {
            format!(
                "Could not set remote SSH permissions on {alias}: {}",
                command_detail(output)
            )
        }
    }
}

fn bootstrap_task(
    task_id: &str,
    host_id: &str,
    host_name: &str,
    status: TaskStatus,
    summary: &str,
    logs: Vec<TaskLog>,
) -> TaskRun {
    TaskRun {
        id: task_id.to_string(),
        host_id: host_id.to_string(),
        host_name: host_name.to_string(),
        action: "Bootstrap SSH key".into(),
        status,
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.to_string(),
        logs,
    }
}

fn run_remote_probe(
    state: &AppState,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> RemoteProbeResult {
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let task_id = format!("task-probe-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let check_output = match alias_result {
        Ok(valid_alias) => ssh::run_ssh_echo_ok(&valid_alias, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {valid_alias} echo ok"),
                format!("Could not start ssh: {error}"),
            )
        }),
        Err(error) => failed_command_output("ssh <invalid-alias> echo ok".into(), error),
    };
    let check_ok = check_output.success() && check_output.stdout.trim() == "ok";
    let check_message = ssh_check_message(&alias, &check_output, check_ok, timeout);
    let mut logs = vec![command_log(
        &task_id,
        1,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &check_message,
        &check_output,
    )];

    if !check_ok {
        update_host_check(state, &alias, false, check_output.duration_ms);
        let task = TaskRun {
            id: task_id.clone(),
            host_id,
            host_name,
            action: "Probe remote system".into(),
            status: TaskStatus::Failed,
            started_at: "now".into(),
            ended_at: Some("now".into()),
            summary: format!("Remote probe skipped because SSH check failed: {check_message}"),
            logs,
        };
        record_task(state, task.clone());
        return RemoteProbeResult {
            host_alias: alias,
            ssh_status: HostStatus::Offline,
            os: "Unknown".into(),
            arch: "Unknown".into(),
            shell: "Unknown".into(),
            path: None,
            path_has_local_bin: false,
            codex_installed: false,
            codex_path: None,
            codex_version: "not installed".into(),
            config_exists: false,
            skills_exists: false,
            skills_count: 0,
            task,
        };
    }
    update_host_check(state, &alias, true, check_output.duration_ms);

    let codex_path_probe_script = codex_path_probe_script();
    let codex_version_probe_script = codex_version_probe_script();
    let commands = vec![
        ("uname -s", "uname -s", TaskLogLevel::Info),
        ("uname -m", "uname -m", TaskLogLevel::Info),
        (
            "echo $SHELL",
            "printf '%s\n' \"${SHELL:-$(getent passwd \"$(id -un)\" 2>/dev/null | cut -d: -f7)}\"",
            TaskLogLevel::Info,
        ),
        ("resolve codex", codex_path_probe_script.as_str(), TaskLogLevel::Warn),
        ("codex --version", codex_version_probe_script.as_str(), TaskLogLevel::Warn),
        ("echo $PATH", "printf '%s\n' \"$PATH\"", TaskLogLevel::Info),
        (
            "check ~/.codex/config.toml",
            "test -f \"$HOME/.codex/config.toml\" && printf yes || printf no",
            TaskLogLevel::Info,
        ),
        (
            "check ~/.codex/skills",
            "test -d \"$HOME/.codex/skills\" && printf yes || printf no",
            TaskLogLevel::Info,
        ),
        (
            "count ~/.codex/skills",
            "if [ -d \"$HOME/.codex/skills\" ]; then find \"$HOME/.codex/skills\" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l; else printf 0; fi",
            TaskLogLevel::Info,
        ),
    ];

    let mut outputs = Vec::new();
    for (index, (label, script, failure_level)) in commands.iter().enumerate() {
        let output = ssh::run_ssh_script(&alias, script, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {alias} {label}"),
                format!("Could not run ssh: {error}"),
            )
        });
        let level = if output.success() {
            TaskLogLevel::Info
        } else {
            (*failure_level).clone()
        };
        let message = if output.success() {
            format!("{label} completed.")
        } else if output.timed_out {
            format!("{label} timed out after {timeout} ms.")
        } else {
            format!("{label} failed: {}", command_detail(&output))
        };
        logs.push(command_log(&task_id, index + 2, level, &message, &output));
        outputs.push(output);
    }

    let os = stdout_or_unknown(outputs.get(0));
    let arch = stdout_or_unknown(outputs.get(1));
    let shell = stdout_or_unknown(outputs.get(2));
    let codex_path = outputs
        .get(3)
        .filter(|output| output.success())
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let codex_installed = codex_path.is_some();
    let codex_version = if codex_installed {
        outputs
            .get(4)
            .filter(|output| output.success())
            .map(|output| output.stdout.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unavailable".into())
    } else {
        "not installed".into()
    };
    let path = outputs
        .get(5)
        .filter(|output| output.success())
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let path_has_local_bin = path_has_local_bin(path.as_deref());
    let config_exists = stdout_yes(outputs.get(6));
    let skills_exists = stdout_yes(outputs.get(7));
    let skills_count = outputs
        .get(8)
        .and_then(|output| output.stdout.trim().parse::<u16>().ok())
        .unwrap_or(0);
    let summary = format!(
        "Probe completed for {alias}: {os}/{arch}, Codex {}.",
        if codex_installed {
            codex_version.as_str()
        } else {
            "not installed"
        }
    );
    let task = TaskRun {
        id: task_id.clone(),
        host_id,
        host_name,
        action: "Probe remote system".into(),
        status: TaskStatus::Success,
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.clone(),
        logs,
    };

    update_host_probe(
        state,
        &alias,
        &os,
        &arch,
        &shell,
        path.clone(),
        path_has_local_bin,
        codex_installed,
        &codex_version,
        config_exists,
        skills_exists,
        skills_count,
    );
    record_task(state, task.clone());

    RemoteProbeResult {
        host_alias: alias,
        ssh_status: HostStatus::Online,
        os,
        arch,
        shell,
        path,
        path_has_local_bin,
        codex_installed,
        codex_path,
        codex_version,
        config_exists,
        skills_exists,
        skills_count,
        task,
    }
}

fn run_remote_manage_codex(
    app: &AppHandle,
    state: &AppState,
    host_alias: String,
    action: RemoteCodexAction,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> RemoteCodexMaintenanceResult {
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let task_id = format!("task-codex-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let action_label = remote_codex_action_label(&action);
    let progress = CodexProgressContext {
        app,
        request_id: request_id.as_deref(),
        host_alias: &alias,
        action: &action,
    };

    emit_remote_codex_progress(
        Some(&progress),
        "ssh-check",
        "running",
        format!("Checking SSH connection to {alias}."),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let check_output = match alias_result {
        Ok(valid_alias) => ssh::run_ssh_echo_ok(&valid_alias, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {valid_alias} echo ok"),
                format!("Could not start ssh: {error}"),
            )
        }),
        Err(error) => failed_command_output("ssh <invalid-alias> echo ok".into(), error),
    };
    let check_ok = check_output.success() && check_output.stdout.trim() == "ok";
    let check_message = ssh_check_message(&alias, &check_output, check_ok, timeout);
    let mut logs = vec![command_log(
        &task_id,
        1,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &check_message,
        &check_output,
    )];
    emit_remote_codex_progress_for_output(
        Some(&progress),
        "ssh-check",
        if check_ok { "success" } else { "failed" },
        &check_message,
        &check_output,
    );

    if !check_ok {
        update_host_check(state, &alias, false, check_output.duration_ms);
        let message = format!("{action_label} skipped because SSH check failed: {check_message}");
        let task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            TaskStatus::Failed,
            &message,
            logs,
        );
        emit_remote_codex_progress(
            Some(&progress),
            "summary",
            "failed",
            message.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        record_task(state, task.clone());
        return RemoteCodexMaintenanceResult {
            host_alias: alias.clone(),
            ok: false,
            action: action.clone(),
            before_version: None,
            after_version: None,
            codex_path: None,
            install_method: None,
            path_changed: false,
            shell_config_path: None,
            backup_path: None,
            message,
            task,
        };
    }
    update_host_check(state, &alias, true, check_output.duration_ms);

    let mut next_log_index = 2;
    let before_path = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "resolve existing codex",
        &codex_path_probe_script(),
        timeout,
        TaskLogLevel::Warn,
        Some(&progress),
    );
    let before_version_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "existing codex --version",
        &codex_version_probe_script(),
        timeout,
        TaskLogLevel::Warn,
        Some(&progress),
    );
    let before_version = output_trimmed(&before_version_output);

    if action == RemoteCodexAction::CheckVersion {
        let path_output = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            "echo $PATH",
            "printf '%s\n' \"$PATH\"",
            timeout,
            TaskLogLevel::Info,
            Some(&progress),
        );
        let codex_path = output_trimmed(&before_path);
        let ok = codex_path.is_some() && before_version.is_some();
        let version_label = before_version
            .clone()
            .unwrap_or_else(|| "not installed".into());
        let message = if ok {
            format!("Codex is available on {alias}: {version_label}.")
        } else {
            format!("Codex is not available on {alias}.")
        };
        let task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            if ok {
                TaskStatus::Success
            } else {
                TaskStatus::Failed
            },
            &message,
            logs,
        );
        update_host_codex_status(
            state,
            &alias,
            ok,
            &version_label,
            path_has_local_bin(output_trimmed(&path_output).as_deref()),
        );
        emit_remote_codex_progress(
            Some(&progress),
            "summary",
            if ok { "success" } else { "failed" },
            message.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        record_task(state, task.clone());
        return RemoteCodexMaintenanceResult {
            host_alias: alias.clone(),
            ok,
            action: action.clone(),
            before_version: before_version.clone(),
            after_version: before_version,
            codex_path,
            install_method: None,
            path_changed: false,
            shell_config_path: None,
            backup_path: None,
            message,
            task,
        };
    }

    let path_repair_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "ensure ~/.local/bin and shell PATH",
        CODEX_PATH_REPAIR_SCRIPT,
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    let path_changed = marker_value(&path_repair_output.stdout, "CODEXHUB_PATH_CHANGED")
        .map(|value| value == "yes")
        .unwrap_or(false);
    let shell_config_path = marker_value(&path_repair_output.stdout, "CODEXHUB_SHELL_CONFIG_PATH");
    let backup_path = marker_value(&path_repair_output.stdout, "CODEXHUB_BACKUP_PATH");

    let mut install_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        action_label,
        CODEX_INSTALL_SCRIPT,
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    if !install_output.success() {
        install_output = run_local_upload_codex_fallback(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            timeout,
            Some(&progress),
        );
    }
    let install_method = marker_value(&install_output.stdout, "CODEXHUB_INSTALL_METHOD")
        .filter(|value| value != "failed");

    let after_path = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "resolve codex after maintenance",
        &codex_path_probe_script(),
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    let after_version_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "codex --version after maintenance",
        &codex_version_probe_script(),
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    let path_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "echo $PATH after maintenance",
        "printf '%s\n' \"$PATH\"",
        timeout,
        TaskLogLevel::Info,
        Some(&progress),
    );

    let codex_path = output_trimmed(&after_path);
    let after_version = output_trimmed(&after_version_output);
    let ok = install_output.success() && codex_path.is_some() && after_version.is_some();
    let version_label = after_version
        .clone()
        .unwrap_or_else(|| "not installed".into());
    let message = if ok {
        format!("{action_label} completed on {alias}: {version_label}.")
    } else {
        format!(
            "{action_label} failed on {alias}: {}",
            command_detail(&install_output)
        )
    };
    let task = codex_maintenance_task(
        &task_id,
        &host_id,
        &host_name,
        action_label,
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &message,
        logs,
    );
    update_host_codex_status(
        state,
        &alias,
        ok,
        &version_label,
        path_has_local_bin(output_trimmed(&path_output).as_deref()),
    );
    emit_remote_codex_progress(
        Some(&progress),
        "summary",
        if ok { "success" } else { "failed" },
        message.clone(),
        install_method.clone(),
        None,
        None,
        None,
        None,
        None,
    );
    record_task(state, task.clone());

    RemoteCodexMaintenanceResult {
        host_alias: alias.clone(),
        ok,
        action: action.clone(),
        before_version,
        after_version,
        codex_path,
        install_method,
        path_changed,
        shell_config_path,
        backup_path,
        message,
        task,
    }
}

fn run_codex_step(
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    label: &str,
    script: &str,
    timeout: u64,
    failure_level: TaskLogLevel,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    emit_remote_codex_progress(
        progress,
        label,
        "running",
        format!("{label} started."),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let output = if let Some(progress) = progress {
        ssh::run_ssh_script_streaming(alias, script, timeout, |event| {
            emit_remote_codex_stream_event(progress, label, event);
        })
    } else {
        ssh::run_ssh_script(alias, script, timeout)
    }
    .unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} {label}"),
            format!("Could not run ssh: {error}"),
        )
    });
    let status = if output.success() {
        "success"
    } else {
        "failed"
    };
    let message = command_step_message(label, &output, timeout);
    emit_remote_codex_progress_for_output(progress, label, status, &message, &output);
    push_command_step_log(
        task_id,
        logs,
        next_log_index,
        label,
        &output,
        timeout,
        failure_level,
    );
    output
}

fn command_step_message(label: &str, output: &ssh::SshCommandOutput, timeout: u64) -> String {
    if output.success() {
        format!("{label} completed.")
    } else if output.timed_out {
        format!("{label} timed out after {timeout} ms.")
    } else {
        format!("{label} failed: {}", command_detail(output))
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_remote_codex_progress(
    progress: Option<&CodexProgressContext<'_>>,
    step: &str,
    status: &str,
    message: String,
    detail: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    timed_out: Option<bool>,
) {
    let Some(progress) = progress else {
        return;
    };
    let Some(request_id) = progress.request_id else {
        return;
    };

    let payload = RemoteCodexProgressEvent {
        request_id: request_id.to_string(),
        host_alias: progress.host_alias.to_string(),
        action: progress.action.clone(),
        step: step.to_string(),
        status: status.to_string(),
        message,
        detail,
        stdout,
        stderr,
        exit_code,
        duration_ms,
        timed_out,
    };
    let _ = progress.app.emit("remote-codex-progress", payload);
}

fn emit_remote_codex_stream_event(
    progress: &CodexProgressContext<'_>,
    step: &str,
    event: ssh::ProcessStreamEvent,
) {
    match event.kind {
        ssh::ProcessStreamKind::Stdout => emit_remote_codex_progress(
            Some(progress),
            step,
            "stdout",
            event.line.clone(),
            None,
            Some(event.line),
            None,
            None,
            Some(event.elapsed_ms),
            None,
        ),
        ssh::ProcessStreamKind::Stderr => emit_remote_codex_progress(
            Some(progress),
            step,
            "stderr",
            event.line.clone(),
            None,
            None,
            Some(event.line),
            None,
            Some(event.elapsed_ms),
            None,
        ),
        ssh::ProcessStreamKind::Heartbeat => emit_remote_codex_progress(
            Some(progress),
            step,
            "heartbeat",
            event.line.clone(),
            Some(step.to_string()),
            None,
            None,
            None,
            Some(event.elapsed_ms),
            None,
        ),
    }
}

fn emit_remote_codex_progress_for_output(
    progress: Option<&CodexProgressContext<'_>>,
    step: &str,
    status: &str,
    message: &str,
    output: &ssh::SshCommandOutput,
) {
    emit_remote_codex_progress(
        progress,
        step,
        status,
        message.to_string(),
        None,
        first_output_line(&output.stdout),
        first_output_line(&output.stderr),
        output.exit_code,
        Some(output.duration_ms),
        Some(output.timed_out),
    );
}

fn first_output_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn push_command_step_log(
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    label: &str,
    output: &ssh::SshCommandOutput,
    timeout: u64,
    failure_level: TaskLogLevel,
) {
    let level = if output.success() {
        TaskLogLevel::Info
    } else {
        failure_level
    };
    let message = command_step_message(label, output, timeout);
    logs.push(command_log(
        task_id,
        *next_log_index,
        level,
        &message,
        output,
    ));
    *next_log_index += 1;
}

fn run_local_upload_codex_fallback(
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let platform_output = run_codex_step(
        alias,
        task_id,
        logs,
        next_log_index,
        "detect Codex native package platform",
        CODEX_NATIVE_PLATFORM_SCRIPT,
        timeout,
        TaskLogLevel::Error,
        progress,
    );
    if !platform_output.success() {
        return platform_output;
    }

    let platform = match marker_value(&platform_output.stdout, "CODEXHUB_NATIVE_PLATFORM") {
        Some(value) => value,
        None => {
            let output = failed_command_output(
                "parse remote Codex native platform".into(),
                "Remote platform probe did not return CODEXHUB_NATIVE_PLATFORM.".into(),
            );
            push_command_step_log(
                task_id,
                logs,
                next_log_index,
                "parse Codex native package platform",
                &output,
                timeout,
                TaskLogLevel::Error,
            );
            emit_remote_codex_progress_for_output(
                progress,
                "parse Codex native package platform",
                "failed",
                "Remote platform probe did not return CODEXHUB_NATIVE_PLATFORM.",
                &output,
            );
            return output;
        }
    };
    let target = match marker_value(&platform_output.stdout, "CODEXHUB_NATIVE_TARGET") {
        Some(value) => value,
        None => {
            let output = failed_command_output(
                "parse remote Codex native target".into(),
                "Remote platform probe did not return CODEXHUB_NATIVE_TARGET.".into(),
            );
            push_command_step_log(
                task_id,
                logs,
                next_log_index,
                "parse Codex native package target",
                &output,
                timeout,
                TaskLogLevel::Error,
            );
            emit_remote_codex_progress_for_output(
                progress,
                "parse Codex native package target",
                "failed",
                "Remote platform probe did not return CODEXHUB_NATIVE_TARGET.",
                &output,
            );
            return output;
        }
    };

    let (package, download_output) =
        download_codex_native_package_locally(&platform, &target, timeout, progress);
    push_command_step_log(
        task_id,
        logs,
        next_log_index,
        "download Codex native package locally",
        &download_output,
        timeout,
        TaskLogLevel::Error,
    );
    let Some(package) = package else {
        return download_output;
    };

    let remote_tarball = format!(
        "/tmp/codexhub-codex-{}-{}.tgz",
        timestamp_millis(),
        package.version
    );
    emit_remote_codex_progress(
        progress,
        "upload Codex native package",
        "running",
        "Uploading Codex native package to remote host.".into(),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let upload_output = if let Some(progress) = progress {
        ssh::upload_file_streaming(
            alias,
            &package.tarball_path,
            &remote_tarball,
            timeout,
            |event| {
                emit_remote_codex_stream_event(progress, "upload Codex native package", event);
            },
        )
    } else {
        ssh::upload_file(alias, &package.tarball_path, &remote_tarball, timeout)
    }
    .unwrap_or_else(|error| {
        failed_command_output(
            format!(
                "scp {} {alias}:{remote_tarball}",
                path_string(&package.tarball_path)
            ),
            format!("Could not upload Codex native package: {error}"),
        )
    });
    emit_remote_codex_progress_for_output(
        progress,
        "upload Codex native package",
        if upload_output.success() {
            "success"
        } else {
            "failed"
        },
        &command_step_message("upload Codex native package", &upload_output, timeout),
        &upload_output,
    );
    push_command_step_log(
        task_id,
        logs,
        next_log_index,
        "upload Codex native package",
        &upload_output,
        timeout,
        TaskLogLevel::Error,
    );
    if !upload_output.success() {
        let _ = fs::remove_dir_all(&package.temp_dir);
        return upload_output;
    }

    let install_script =
        codex_install_uploaded_package_script(&remote_tarball, &package.version, &package.target);
    let install_output = run_codex_step(
        alias,
        task_id,
        logs,
        next_log_index,
        "install uploaded Codex native package",
        &install_script,
        timeout,
        TaskLogLevel::Error,
        progress,
    );
    let _ = fs::remove_dir_all(&package.temp_dir);
    install_output
}

fn download_codex_native_package_locally(
    platform: &str,
    target: &str,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> (Option<LocalCodexNativePackage>, ssh::SshCommandOutput) {
    let temp_dir = env::temp_dir().join(format!("codexhub-codex-native-{}", timestamp_millis()));
    if let Err(error) = fs::create_dir_all(&temp_dir) {
        return (
            None,
            failed_command_output(
                "create local Codex package temp directory".into(),
                format!("Could not create local temp directory: {error}"),
            ),
        );
    }

    let metadata_path = temp_dir.join("codex-metadata.json");
    let metadata_url = "https://registry.npmmirror.com/@openai/codex";
    let metadata_output = local_curl_download(
        metadata_url,
        &metadata_path,
        "download @openai/codex metadata from npmmirror",
        timeout,
        progress,
    );
    if !metadata_output.success() {
        let _ = fs::remove_dir_all(&temp_dir);
        return (None, metadata_output);
    }

    let metadata = match fs::read_to_string(&metadata_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return (
                None,
                failed_command_output(
                    "read local @openai/codex metadata".into(),
                    format!("Could not read downloaded npmmirror metadata: {error}"),
                ),
            );
        }
    };
    let (version, tarball_url) = match parse_npmmirror_native_metadata(&metadata, platform) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return (
                None,
                failed_command_output("parse local @openai/codex metadata".into(), error),
            );
        }
    };
    if !is_safe_codex_package_version(&version) {
        let _ = fs::remove_dir_all(&temp_dir);
        return (
            None,
            failed_command_output(
                "validate local @openai/codex package version".into(),
                format!("npmmirror returned an unsafe Codex package version: {version}"),
            ),
        );
    }

    let tarball_path = temp_dir.join(format!("codex-{version}-{platform}.tgz"));
    let tarball_output = local_curl_download(
        &tarball_url,
        &tarball_path,
        "download @openai/codex native package from npmmirror",
        timeout,
        progress,
    );
    if !tarball_output.success() {
        let _ = fs::remove_dir_all(&temp_dir);
        return (None, tarball_output);
    }

    let stdout = format!(
        "CODEXHUB_LOCAL_PACKAGE_PLATFORM={platform}\nCODEXHUB_LOCAL_PACKAGE_TARGET={target}\nCODEXHUB_LOCAL_PACKAGE_VERSION={version}\nCODEXHUB_LOCAL_PACKAGE_TARBALL={tarball_url}\n"
    );
    let output = ssh::SshCommandOutput {
        command: "local npmmirror native package download".into(),
        stdout,
        stderr: tarball_output.stderr.clone(),
        exit_code: Some(0),
        duration_ms: metadata_output.duration_ms + tarball_output.duration_ms,
        timed_out: false,
    };

    (
        Some(LocalCodexNativePackage {
            version,
            target: target.to_string(),
            tarball_path,
            temp_dir,
        }),
        output,
    )
}

fn local_curl_download(
    url: &str,
    output_path: &Path,
    label: &str,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let timeout_secs = ((timeout + 999) / 1000).clamp(1, 120).to_string();
    let args = vec![
        "-fsSL".into(),
        "--connect-timeout".into(),
        "15".into(),
        "--max-time".into(),
        timeout_secs,
        url.into(),
        "-o".into(),
        output_path.to_string_lossy().to_string(),
    ];
    let command = format!("curl -fsSL {url} -o {}", path_string(output_path));
    emit_remote_codex_progress(
        progress,
        label,
        "running",
        format!("{label} started."),
        Some(url.into()),
        None,
        None,
        None,
        None,
        None,
    );
    let output = if let Some(progress) = progress {
        ssh::run_local_process_streaming("curl", &args, &command, timeout, |event| {
            emit_remote_codex_stream_event(progress, label, event);
        })
    } else {
        ssh::run_local_process("curl", &args, &command, timeout)
    }
    .unwrap_or_else(|error| {
        failed_command_output(
            label.to_string(),
            format!("Could not start local curl: {error}"),
        )
    });
    emit_remote_codex_progress_for_output(
        progress,
        label,
        if output.success() {
            "success"
        } else {
            "failed"
        },
        &command_step_message(label, &output, timeout),
        &output,
    );
    output
}

fn parse_npmmirror_native_metadata(
    metadata: &str,
    platform: &str,
) -> Result<(String, String), String> {
    let lower = metadata.to_ascii_lowercase();
    if lower.contains("<html")
        || lower.contains("authentication is required")
        || lower.contains("net2.zju.edu.cn")
        || lower.contains("captive")
        || lower.contains("portal")
    {
        return Err(
            "npmmirror metadata response was HTML instead of JSON; the network may require captive portal authentication before downloads can work."
                .into(),
        );
    }

    let data: serde_json::Value = serde_json::from_str(metadata).map_err(|error| {
        format!(
            "npmmirror metadata response was not JSON; the network may require captive portal authentication before downloads can work: {error}"
        )
    })?;
    let latest = data
        .get("dist-tags")
        .and_then(|value| value.get("latest"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "npmmirror metadata did not include dist-tags.latest for @openai/codex.".to_string()
        })?;
    let package_key = format!("{latest}-{platform}");
    let package = data
        .get("versions")
        .and_then(|value| value.get(&package_key))
        .ok_or_else(|| {
            format!("npmmirror metadata did not include package version {package_key}.")
        })?;
    let tarball = package
        .get("dist")
        .and_then(|value| value.get("tarball"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.starts_with("https://registry.npmmirror.com/"))
        .ok_or_else(|| {
            format!("npmmirror metadata returned an unexpected tarball URL for {package_key}.")
        })?;

    Ok((latest.to_string(), tarball.to_string()))
}

fn is_safe_codex_package_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+'))
}

fn codex_install_uploaded_package_script(
    remote_tarball: &str,
    version: &str,
    target: &str,
) -> String {
    r#"set -u
export CODEX_INSTALL_DIR="$HOME/.local/bin"
export CODEX_HOME="$HOME/.codex"
export PATH="$HOME/.local/bin:$PATH"
remote_tarball=__REMOTE_TARBALL__
version=__VERSION__
target=__TARGET__
tmp_dir="${TMPDIR:-/tmp}/codexhub-upload-install.$$"
stage_dir=""
mkdir -p "$CODEX_INSTALL_DIR" "$CODEX_HOME/packages/standalone/releases" "$tmp_dir"
trap 'rm -rf "$tmp_dir"; [ -n "${stage_dir:-}" ] && rm -rf "$stage_dir"; rm -f "$remote_tarball"' EXIT HUP INT TERM

if [ ! -s "$remote_tarball" ]; then
  printf 'Uploaded Codex native package is missing or empty: %s\n' "$remote_tarball" >&2
  exit 2
fi
if ! tar -tzf "$remote_tarball" >/dev/null 2>&1; then
  printf 'Uploaded Codex native package is not a readable gzip tarball.\n' >&2
  exit 2
fi
if tar -tzf "$remote_tarball" | grep -Eq '(^|/)\.\.(/|$)|^/'; then
  printf 'Uploaded Codex native package archive contains unsafe paths.\n' >&2
  exit 2
fi

extract_dir="$tmp_dir/native-extract"
release_dir="$CODEX_HOME/packages/standalone/releases/$version"
stage_dir="$release_dir.tmp.$$"
rm -rf "$extract_dir" "$stage_dir"
mkdir -p "$extract_dir" "$stage_dir"
tar -xzf "$remote_tarball" -C "$extract_dir"
vendor_dir="$extract_dir/package/vendor/$target"
if [ ! -x "$vendor_dir/bin/codex" ]; then
  printf 'Uploaded Codex native package did not contain vendor/%s/bin/codex.\n' "$target" >&2
  exit 2
fi

cp -R "$vendor_dir/." "$stage_dir/"
chmod 0755 "$stage_dir/bin/codex"
[ -f "$stage_dir/codex-path/rg" ] && chmod 0755 "$stage_dir/codex-path/rg"
[ -f "$stage_dir/codex-resources/bwrap" ] && chmod 0755 "$stage_dir/codex-resources/bwrap"
rm -rf "$release_dir"
mv "$stage_dir" "$release_dir"
stage_dir=""
ln -sfn "$release_dir" "$CODEX_HOME/packages/standalone/current"
ln -sfn "$release_dir/bin/codex" "$CODEX_INSTALL_DIR/codex"
printf 'CODEXHUB_INSTALL_METHOD=npm-mirror-native-local-upload\n'
"#
    .replace("__REMOTE_TARBALL__", &shell_single_quote(remote_tarball))
    .replace("__VERSION__", &shell_single_quote(version))
    .replace("__TARGET__", &shell_single_quote(target))
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn output_trimmed(output: &ssh::SshCommandOutput) -> Option<String> {
    output
        .success()
        .then(|| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn marker_value(stdout: &str, marker: &str) -> Option<String> {
    let prefix = format!("{marker}=");
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn remote_codex_action_label(action: &RemoteCodexAction) -> &'static str {
    match action {
        RemoteCodexAction::CheckVersion => "Check Codex version",
        RemoteCodexAction::Install => "Install Codex",
        RemoteCodexAction::Update => "Update Codex",
    }
}

fn codex_maintenance_task(
    task_id: &str,
    host_id: &str,
    host_name: &str,
    action: &str,
    status: TaskStatus,
    summary: &str,
    logs: Vec<TaskLog>,
) -> TaskRun {
    TaskRun {
        id: task_id.to_string(),
        host_id: host_id.to_string(),
        host_name: host_name.to_string(),
        action: action.to_string(),
        status,
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.to_string(),
        logs,
    }
}

fn record_task(state: &AppState, task: TaskRun) {
    state
        .tasks
        .lock()
        .expect("tasks mutex poisoned")
        .insert(0, task);
}

fn update_host_check(state: &AppState, alias: &str, ok: bool, duration_ms: u64) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = if ok {
            HostStatus::Online
        } else {
            HostStatus::Offline
        };
        host.latency_ms = if ok { Some(duration_ms) } else { None };
        if ok {
            host.last_seen = "just now".into();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_host_probe(
    state: &AppState,
    alias: &str,
    os: &str,
    arch: &str,
    shell: &str,
    path: Option<String>,
    path_has_local_bin: bool,
    codex_installed: bool,
    codex_version: &str,
    config_exists: bool,
    skills_exists: bool,
    skills_count: u16,
) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = HostStatus::Online;
        host.os = os.to_string();
        host.arch = arch.to_string();
        host.shell = shell.to_string();
        host.path = path;
        host.path_has_local_bin = Some(path_has_local_bin);
        host.codex_installed = codex_installed;
        host.codex_version = codex_version.to_string();
        host.config_exists = Some(config_exists);
        host.skills_exists = Some(skills_exists);
        host.skills_count = Some(skills_count);
        host.last_seen = "just now".into();
    }
}

fn update_host_codex_status(
    state: &AppState,
    alias: &str,
    codex_installed: bool,
    codex_version: &str,
    path_has_local_bin: bool,
) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = HostStatus::Online;
        host.codex_installed = codex_installed;
        host.codex_version = codex_version.to_string();
        host.path_has_local_bin = Some(path_has_local_bin);
        host.last_seen = "just now".into();
    }
}

fn host_name_for_alias(state: &AppState, alias: &str) -> String {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .map(|host| host.name.clone())
        .unwrap_or_else(|| alias.to_string())
}

fn host_id_for_alias(state: &AppState, alias: &str) -> String {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .map(|host| host.id.clone())
        .unwrap_or_else(|| discovered_host_id(alias))
}

fn failed_command_output(command: String, message: String) -> ssh::SshCommandOutput {
    ssh::SshCommandOutput {
        command,
        stdout: String::new(),
        stderr: message,
        exit_code: None,
        duration_ms: 0,
        timed_out: false,
    }
}

fn command_log(
    task_id: &str,
    index: usize,
    level: TaskLogLevel,
    message: &str,
    output: &ssh::SshCommandOutput,
) -> TaskLog {
    TaskLog {
        id: format!("{task_id}-log-{index}"),
        task_run_id: task_id.to_string(),
        level,
        timestamp: "now".into(),
        message: message.to_string(),
        command: Some(output.command.clone()),
        stdout: Some(output.stdout.clone()),
        stderr: Some(output.stderr.clone()),
        exit_code: output.exit_code,
        duration_ms: Some(output.duration_ms),
        timed_out: Some(output.timed_out),
    }
}

fn basic_log(task_id: &str, index: usize, level: TaskLogLevel, message: &str) -> TaskLog {
    TaskLog {
        id: format!("{task_id}-log-{index}"),
        task_run_id: task_id.to_string(),
        level,
        timestamp: "now".into(),
        message: message.to_string(),
        command: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        duration_ms: None,
        timed_out: None,
    }
}

fn ssh_check_message(
    alias: &str,
    output: &ssh::SshCommandOutput,
    ok: bool,
    timeout_ms: u64,
) -> String {
    if ok {
        format!("SSH connection to {alias} returned ok.")
    } else if output.timed_out {
        format!("SSH connection to {alias} timed out after {timeout_ms} ms.")
    } else {
        format!(
            "SSH connection to {alias} failed: {}",
            ssh_failure_hint(output)
        )
    }
}

fn ssh_failure_hint(output: &ssh::SshCommandOutput) -> String {
    let detail = command_detail(output);
    let lower = detail.to_ascii_lowercase();
    if lower.contains("host key verification failed") {
        return format!("{detail}. The host key may have changed; CodexHub accepts only first-time new host keys automatically.");
    }
    if lower.contains("permission denied") || lower.contains("no supported authentication methods")
    {
        return format!("{detail}. Use one-time password setup when adding the host so CodexHub can install your local public key, or configure SSH key/agent auth manually.");
    }
    detail
}

fn command_detail(output: &ssh::SshCommandOutput) -> String {
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        return stderr.lines().next().unwrap_or(stderr).to_string();
    }
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return stdout.lines().next().unwrap_or(stdout).to_string();
    }
    match output.exit_code {
        Some(code) => format!("exit code {code}"),
        None => "process did not start".into(),
    }
}

fn stdout_or_unknown(output: Option<&ssh::SshCommandOutput>) -> String {
    output
        .filter(|item| item.success())
        .map(|item| item.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Unknown".into())
}

fn stdout_yes(output: Option<&ssh::SshCommandOutput>) -> bool {
    output
        .filter(|item| item.success())
        .map(|item| item.stdout.trim().eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
}

fn path_has_local_bin(path: Option<&str>) -> bool {
    path.unwrap_or_default()
        .split(':')
        .any(|segment| segment == "~/.local/bin" || segment.ends_with("/.local/bin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_resolver_checks_login_paths_and_package_metadata() {
        let path_script = codex_path_probe_script();
        let version_script = codex_version_probe_script();

        assert!(path_script.contains("package_version_for_candidate"));
        assert!(path_script.contains("$login_shell\" -lc"));
        assert!(path_script.contains("\"$HOME/.nvm/versions/node\"/*/bin/codex"));
        assert!(path_script.contains("\"$HOME/.local/share/pnpm/codex\""));
        assert!(path_script.contains("\"$HOME/node_modules/.bin/codex\""));
        assert!(path_script.contains("</dev/null"));
        assert!(version_script.ends_with("printf '%s\\n' \"$best_version\""));
    }

    #[test]
    fn ssh_failure_hint_explains_host_key_and_password_cases() {
        let host_key_output = ssh::SshCommandOutput {
            command: "ssh lab echo ok".into(),
            stdout: String::new(),
            stderr: "Host key verification failed.".into(),
            exit_code: Some(255),
            duration_ms: 10,
            timed_out: false,
        };
        let password_output = ssh::SshCommandOutput {
            command: "ssh lab echo ok".into(),
            stdout: String::new(),
            stderr: "Permission denied (publickey,password).".into(),
            exit_code: Some(255),
            duration_ms: 10,
            timed_out: false,
        };

        assert!(ssh_failure_hint(&host_key_output).contains("first-time new host keys"));
        assert!(ssh_failure_hint(&password_output).contains("one-time password setup"));
    }

    #[test]
    fn remote_codex_action_serializes_as_kebab_case() {
        assert_eq!(
            serde_json::to_string(&RemoteCodexAction::CheckVersion).expect("serialize"),
            "\"check-version\""
        );
        assert_eq!(
            serde_json::to_string(&RemoteCodexAction::Install).expect("serialize"),
            "\"install\""
        );
        assert_eq!(
            serde_json::from_str::<RemoteCodexAction>("\"update\"").expect("deserialize"),
            RemoteCodexAction::Update
        );
    }

    #[test]
    fn remote_codex_progress_event_serializes_camel_case() {
        let event = RemoteCodexProgressEvent {
            request_id: "req-1".into(),
            host_alias: "lab".into(),
            action: RemoteCodexAction::Install,
            step: "Install Codex".into(),
            status: "stdout".into(),
            message: "downloading".into(),
            detail: Some("detail".into()),
            stdout: Some("line".into()),
            stderr: None,
            exit_code: Some(0),
            duration_ms: Some(42),
            timed_out: Some(false),
        };
        let json = serde_json::to_string(&event).expect("serialize progress");

        assert!(json.contains("\"requestId\":\"req-1\""));
        assert!(json.contains("\"hostAlias\":\"lab\""));
        assert!(json.contains("\"exitCode\":0"));
        assert!(json.contains("\"durationMs\":42"));
    }

    #[test]
    fn codex_install_script_uses_user_install_dir_and_mirror_fallback() {
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEX_INSTALL_DIR=\"$HOME/.local/bin\""));
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEX_HOME=\"$HOME/.codex\""));
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEX_NON_INTERACTIVE=1"));
        assert!(CODEX_INSTALL_SCRIPT.contains("https://chatgpt.com/codex/install.sh"));
        assert!(CODEX_INSTALL_SCRIPT.contains("registry=https://registry.npmmirror.com"));
        assert!(CODEX_INSTALL_SCRIPT.contains("https://registry.npmmirror.com/@openai/codex"));
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native"));
        assert!(
            CODEX_INSTALL_SCRIPT.contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native-insecure-tls")
        );
        assert!(CODEX_INSTALL_SCRIPT
            .contains("CodexHub will not disable TLS verification for the official installer"));
        assert!(CODEX_INSTALL_SCRIPT.contains("Insecure TLS fallback is limited to npmmirror URLs"));
        assert!(
            CODEX_INSTALL_SCRIPT.contains("npmmirror metadata response was HTML instead of JSON")
        );
        assert!(CODEX_INSTALL_SCRIPT.contains("not a readable gzip tarball"));
        assert!(CODEX_INSTALL_SCRIPT.contains("archive contains unsafe paths"));
        assert!(CODEX_INSTALL_SCRIPT.contains("curl -k -fsSL"));
        assert!(CODEX_INSTALL_SCRIPT.contains("wget --no-check-certificate"));
        assert!(CODEX_INSTALL_SCRIPT.contains("vendor/$target"));
        assert!(CODEX_INSTALL_SCRIPT.contains("ln -sfn \"$release_dir/bin/codex\""));
        assert!(CODEX_INSTALL_SCRIPT.contains("command -v npm"));
        assert!(!CODEX_INSTALL_SCRIPT.contains("sudo"));
        assert!(!CODEX_INSTALL_SCRIPT.contains("chown"));
        assert!(!CODEX_INSTALL_SCRIPT.contains("/usr/local/bin"));
    }

    #[test]
    fn local_npmmirror_metadata_parser_detects_captive_portal_html() {
        let error = parse_npmmirror_native_metadata(
            r#"<html><body>Authentication is required. https://net2.zju.edu.cn/index_85.html</body></html>"#,
            "linux-x64",
        )
        .expect_err("captive portal should be rejected");

        assert!(error.contains("HTML instead of JSON"));
        assert!(error.contains("captive portal"));
    }

    #[test]
    fn local_npmmirror_metadata_parser_extracts_platform_tarball() {
        let metadata = r#"{
          "dist-tags": { "latest": "0.142.2" },
          "versions": {
            "0.142.2-linux-x64": {
              "dist": {
                "tarball": "https://registry.npmmirror.com/@openai/codex/-/codex-0.142.2-linux-x64.tgz"
              }
            }
          }
        }"#;

        let (version, tarball) =
            parse_npmmirror_native_metadata(metadata, "linux-x64").expect("parse metadata");

        assert_eq!(version, "0.142.2");
        assert_eq!(
            tarball,
            "https://registry.npmmirror.com/@openai/codex/-/codex-0.142.2-linux-x64.tgz"
        );
    }

    #[test]
    fn uploaded_codex_package_script_uses_user_install_dir_without_wrapper() {
        let script = codex_install_uploaded_package_script(
            "/tmp/codexhub-codex-1-0.142.2.tgz",
            "0.142.2",
            "x86_64-unknown-linux-musl",
        );

        assert!(script.contains("CODEX_INSTALL_DIR=\"$HOME/.local/bin\""));
        assert!(script.contains("CODEX_HOME=\"$HOME/.codex\""));
        assert!(script.contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native-local-upload"));
        assert!(script.contains("ln -sfn \"$release_dir/bin/codex\" \"$CODEX_INSTALL_DIR/codex\""));
        assert!(script.contains("vendor/$target"));
        assert!(script.contains("archive contains unsafe paths"));
        assert!(!script.contains("sudo"));
        assert!(!script.contains("/usr/local/bin"));
        assert!(!script.contains("wrapper"));
    }

    #[test]
    fn codex_path_repair_script_is_managed_idempotent_and_backed_up() {
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("mkdir -p \"$local_bin\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("# >>> CodexHub managed PATH"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("# <<< CodexHub managed PATH"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("CODEXHUB_PATH_CHANGED=no"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("CODEXHUB_PATH_CHANGED=yes"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("cp -p \"$shell_config\" \"$backup_path\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("grep -F \"$path_line\""));
        assert!(!CODEX_PATH_REPAIR_SCRIPT.contains("sudo"));

        let backup_index = CODEX_PATH_REPAIR_SCRIPT
            .find("cp -p \"$shell_config\" \"$backup_path\"")
            .expect("backup command");
        let append_index = CODEX_PATH_REPAIR_SCRIPT
            .find(">>\"$shell_config\"")
            .expect("append command");
        assert!(backup_index < append_index);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_health,
            get_settings,
            save_settings,
            get_ssh_status,
            generate_ed25519_key,
            list_ssh_config_hosts,
            upsert_ssh_config_host,
            delete_ssh_config_host,
            list_hosts,
            refresh_discovered_hosts,
            add_host,
            update_host,
            delete_host,
            test_ssh_connection,
            ssh_check,
            bootstrap_ssh_host,
            bootstrap_existing_ssh_host,
            remote_probe_codex,
            remote_manage_codex,
            list_profiles,
            apply_profile,
            list_tasks,
            list_skill_packs
        ])
        .run(tauri::generate_context!())
        .expect("error while running CodexHub");
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn settings_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("settings.json")
}

fn read_settings(app: &AppHandle) -> AppSettings {
    let path = settings_path(app);
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<AppSettings>(&content).ok())
        .unwrap_or_default()
}

fn write_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

fn mock_hosts() -> Vec<Host> {
    vec![
        Host {
            id: "mac-studio-lab".into(),
            name: "Mac Studio Lab".into(),
            host_alias: "mac-studio-lab".into(),
            source: "mock".into(),
            address: "10.0.8.12".into(),
            port: 22,
            username: "jurio".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Online,
            os: "macOS 15.5".into(),
            arch: "arm64".into(),
            shell: "/bin/zsh".into(),
            path: Some("/Users/jurio/.local/bin:/usr/local/bin:/usr/bin".into()),
            path_has_local_bin: Some(true),
            codex_installed: true,
            codex_version: "0.32.0".into(),
            config_exists: Some(true),
            skills_exists: Some(true),
            skills_count: Some(5),
            profile_id: Some("research-default".into()),
            skill_pack_ids: vec!["paper-review".into(), "tauri-builder".into()],
            tags: vec!["local".into(), "gpu".into()],
            last_seen: "2 min ago".into(),
            latency_ms: Some(18),
        },
        Host {
            id: "win-workstation".into(),
            name: "Windows Workstation".into(),
            host_alias: "win-workstation".into(),
            source: "mock".into(),
            address: "192.168.31.42".into(),
            port: 22,
            username: "pc".into(),
            auth_method: AuthMethod::Agent,
            status: HostStatus::Unknown,
            os: "Windows 11 Pro".into(),
            arch: "x86_64".into(),
            shell: "Unknown".into(),
            path: None,
            path_has_local_bin: None,
            codex_installed: false,
            codex_version: "pending".into(),
            config_exists: None,
            skills_exists: None,
            skills_count: None,
            profile_id: Some("safe-editing".into()),
            skill_pack_ids: vec!["tauri-builder".into()],
            tags: vec!["desktop".into(), "primary".into()],
            last_seen: "not tested".into(),
            latency_ms: None,
        },
        Host {
            id: "linux-runner".into(),
            name: "Linux Runner".into(),
            host_alias: "linux-runner".into(),
            source: "mock".into(),
            address: "172.20.4.8".into(),
            port: 2222,
            username: "codex".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Offline,
            os: "Ubuntu 24.04 LTS".into(),
            arch: "x86_64".into(),
            shell: "/bin/bash".into(),
            path: Some("/home/codex/.local/bin:/usr/local/bin:/usr/bin".into()),
            path_has_local_bin: Some(true),
            codex_installed: true,
            codex_version: "0.31.1".into(),
            config_exists: Some(false),
            skills_exists: Some(true),
            skills_count: Some(1),
            profile_id: None,
            skill_pack_ids: vec!["paper-review".into()],
            tags: vec!["remote".into(), "ci".into()],
            last_seen: "yesterday".into(),
            latency_ms: None,
        },
    ]
}

fn mock_profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "research-default".into(),
            name: "Research Default".into(),
            description: "Balanced model and approval policy for literature review, repo browsing, and report drafting.".into(),
            model: "gpt-5-codex".into(),
            approval_policy: "on-request".into(),
            sandbox_mode: "workspace-write".into(),
            updated_at: "2026-06-24 22:10".into(),
            host_ids: vec!["mac-studio-lab".into()],
        },
        Profile {
            id: "safe-editing".into(),
            name: "Safe Editing".into(),
            description: "Conservative profile for protected repos, narrow write scope, and explicit publish steps.".into(),
            model: "gpt-5-codex".into(),
            approval_policy: "on-failure".into(),
            sandbox_mode: "workspace-write".into(),
            updated_at: "2026-06-23 18:35".into(),
            host_ids: vec!["win-workstation".into()],
        },
        Profile {
            id: "diagnostics".into(),
            name: "Diagnostics".into(),
            description: "Read-mostly profile for host checks, logs, and environment inspection.".into(),
            model: "gpt-5-mini".into(),
            approval_policy: "never".into(),
            sandbox_mode: "read-only".into(),
            updated_at: "2026-06-21 09:42".into(),
            host_ids: Vec::new(),
        },
    ]
}

fn mock_skill_packs() -> Vec<SkillPack> {
    vec![
        SkillPack {
            id: "paper-review".into(),
            name: "Paper Review".into(),
            version: "0.4.1".into(),
            description: "Summarize papers, extract claims, and prepare structured reading notes."
                .into(),
            source: "~/.codex/skills/paper-review".into(),
            skill_count: 5,
            enabled: true,
            updated_at: "2026-06-24".into(),
        },
        SkillPack {
            id: "tauri-builder".into(),
            name: "Tauri Builder".into(),
            version: "0.2.0".into(),
            description:
                "Scaffold, test, and package Tauri desktop features with React and Rust boundaries."
                    .into(),
            source: "./skills/tauri-builder".into(),
            skill_count: 3,
            enabled: true,
            updated_at: "2026-06-20".into(),
        },
        SkillPack {
            id: "windows-diagnostics".into(),
            name: "Windows Diagnostics".into(),
            version: "0.1.5".into(),
            description:
                "Collect reproducible PowerShell checks for network, shell, and toolchain issues."
                    .into(),
            source: "./skills/windows-diagnostics".into(),
            skill_count: 4,
            enabled: false,
            updated_at: "2026-06-18".into(),
        },
    ]
}

fn mock_tasks() -> Vec<TaskRun> {
    vec![
        TaskRun {
            id: "task-1042".into(),
            host_id: "mac-studio-lab".into(),
            host_name: "Mac Studio Lab".into(),
            action: "Apply profile".into(),
            status: TaskStatus::Success,
            started_at: "2026-06-25 09:14".into(),
            ended_at: Some("2026-06-25 09:15".into()),
            summary:
                "Research Default rendered to ~/.codex/config.toml with backup codexhub-1042.toml."
                    .into(),
            logs: vec![
                basic_log(
                    "task-1042",
                    1,
                    TaskLogLevel::Info,
                    "Opened SFTP session and created remote backup.",
                ),
                basic_log(
                    "task-1042",
                    2,
                    TaskLogLevel::Info,
                    "Rendered profile preview matched expected TOML sections.",
                ),
            ],
        },
        TaskRun {
            id: "task-1039".into(),
            host_id: "linux-runner".into(),
            host_name: "Linux Runner".into(),
            action: "Test SSH connection".into(),
            status: TaskStatus::Failed,
            started_at: "2026-06-24 22:02".into(),
            ended_at: Some("2026-06-24 22:02".into()),
            summary:
                "Connection timed out. Check VPN route or host firewall before applying profiles."
                    .into(),
            logs: vec![
                basic_log(
                    "task-1039",
                    1,
                    TaskLogLevel::Warn,
                    "Mock check marks linux-runner offline for UI validation.",
                ),
                basic_log(
                    "task-1039",
                    2,
                    TaskLogLevel::Error,
                    "Connection timeout is simulated; no SSH socket was opened.",
                ),
            ],
        },
        TaskRun {
            id: "task-1035".into(),
            host_id: "win-workstation".into(),
            host_name: "Windows Workstation".into(),
            action: "Sync skill pack".into(),
            status: TaskStatus::Queued,
            started_at: "2026-06-24 18:25".into(),
            ended_at: None,
            summary: "Queued Paper Review skill pack for the next available SSH session.".into(),
            logs: vec![basic_log(
                "task-1035",
                1,
                TaskLogLevel::Info,
                "Task created from mock backend reservation.",
            )],
        },
        TaskRun {
            id: "task-1031".into(),
            host_id: "win-workstation".into(),
            host_name: "Windows Workstation".into(),
            action: "Preview profile".into(),
            status: TaskStatus::Running,
            started_at: "2026-06-24 18:10".into(),
            ended_at: None,
            summary: "Generating a profile diff preview for the safe editing profile.".into(),
            logs: vec![basic_log(
                "task-1031",
                1,
                TaskLogLevel::Info,
                "Mock worker is holding this run in progress for UI coverage.",
            )],
        },
    ]
}
