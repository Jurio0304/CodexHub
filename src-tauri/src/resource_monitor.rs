use crate::ssh;
use chrono::{DateTime, Local};
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{mpsc, Arc},
    thread,
};
use ts_rs::TS;

const RESOURCE_SAMPLE_CONCURRENCY: usize = 3;
const DEFAULT_RESOURCE_SAMPLE_TIMEOUT_MS: u64 = 30_000;
const MAX_RESOURCE_SAMPLE_TIMEOUT_MS: u64 = 60_000;

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostResourceBatchResultDto")]
pub(crate) struct HostResourceBatchResult {
    checked_at: String,
    snapshots: Vec<HostResourceSnapshot>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostResourceProgressEventDto")]
pub(crate) struct HostResourceProgressEvent {
    pub(crate) request_id: String,
    pub(crate) snapshot: HostResourceSnapshot,
}

impl HostResourceBatchResult {
    pub(crate) fn outcome_counts(&self) -> (usize, usize, usize) {
        let failed = self
            .snapshots
            .iter()
            .filter(|snapshot| matches!(snapshot.status, HostResourceStatus::Failed))
            .count();
        let partial = self
            .snapshots
            .iter()
            .filter(|snapshot| matches!(snapshot.status, HostResourceStatus::Partial))
            .count();
        (self.snapshots.len(), partial, failed)
    }

    pub(crate) fn snapshots(&self) -> &[HostResourceSnapshot] {
        &self.snapshots
    }
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostResourceSnapshotDto")]
pub(crate) struct HostResourceSnapshot {
    pub(crate) host_alias: String,
    status: HostResourceStatus,
    pub(crate) ssh_status: HostResourceSshStatus,
    pub(crate) timed_out: bool,
    sampled_at: String,
    #[ts(type = "number | null")]
    latency_ms: Option<u64>,
    error: Option<String>,
    cpu: Option<CpuSnapshot>,
    memory: Option<MemorySnapshot>,
    storage: Option<StorageSnapshot>,
    gpu_tool: String,
    gpus: Vec<GpuSnapshot>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "HostResourceStatusDto")]
pub(crate) enum HostResourceStatus {
    Ok,
    Partial,
    Failed,
}

impl HostResourceSnapshot {
    pub(crate) fn status(&self) -> &HostResourceStatus {
        &self.status
    }

    pub(crate) fn task_log_summary(&self) -> String {
        let duration = self
            .latency_ms
            .map(|value| format!(" in {value} ms"))
            .unwrap_or_default();
        match &self.status {
            HostResourceStatus::Ok => format!(
                "Host {}: resource sampling succeeded{duration}; {} GPU(s) detected.",
                self.host_alias,
                self.gpus.len()
            ),
            HostResourceStatus::Partial => format!(
                "Host {}: resource sampling completed with partial data{duration}; {} GPU(s) detected.",
                self.host_alias,
                self.gpus.len()
            ),
            HostResourceStatus::Failed => {
                let detail = self
                    .error
                    .as_deref()
                    .map(compact_log_detail)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Unknown resource sampling error.".into());
                format!("Host {}: resource sampling failed: {detail}", self.host_alias)
            }
        }
    }
}

fn compact_log_detail(value: &str) -> String {
    const MAX_CHARS: usize = 240;
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX_CHARS {
        return compact;
    }
    format!(
        "{}...",
        compact.chars().take(MAX_CHARS - 3).collect::<String>()
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "HostResourceSshStatusDto")]
pub(crate) enum HostResourceSshStatus {
    Online,
    Offline,
    Unknown,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "CpuSnapshotDto")]
pub(crate) struct CpuSnapshot {
    usage_percent: Option<f64>,
    load1: Option<f64>,
    load5: Option<f64>,
    load15: Option<f64>,
    cores: Option<u16>,
    model: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "MemorySnapshotDto")]
pub(crate) struct MemorySnapshot {
    #[ts(type = "number | null")]
    total_bytes: Option<u64>,
    #[ts(type = "number | null")]
    available_bytes: Option<u64>,
    used_percent: Option<f64>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "StorageSnapshotDto")]
pub(crate) struct StorageSnapshot {
    #[ts(type = "number")]
    total_bytes: u64,
    #[ts(type = "number")]
    available_bytes: u64,
    used_percent: f64,
    filesystems: Vec<StorageFilesystemSnapshot>,
    sampled_at: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "StorageFilesystemSnapshotDto")]
pub(crate) struct StorageFilesystemSnapshot {
    #[ts(type = "number")]
    total_bytes: u64,
    #[ts(type = "number")]
    available_bytes: u64,
    source: String,
    filesystem_type: String,
    mount_points: Vec<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "GpuSnapshotDto")]
pub(crate) struct GpuSnapshot {
    vendor: GpuVendor,
    index: Option<String>,
    uuid: Option<String>,
    name: String,
    status: GpuStatus,
    memory_mode: GpuMemoryMode,
    utilization_percent: Option<f64>,
    #[ts(type = "number | null")]
    memory_used_bytes: Option<u64>,
    #[ts(type = "number | null")]
    memory_total_bytes: Option<u64>,
    temperature_c: Option<f64>,
    power_watts: Option<f64>,
    driver_version: Option<String>,
    processes: Vec<GpuProcessSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "GpuMemoryModeDto")]
pub(crate) enum GpuMemoryMode {
    Dedicated,
    Unified,
    Unknown,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "GpuProcessSnapshotDto")]
pub(crate) struct GpuProcessSnapshot {
    gpu_uuid: Option<String>,
    pid: Option<u32>,
    name: String,
    cpu_usage_percent: Option<f64>,
    #[ts(type = "number | null")]
    used_memory_bytes: Option<u64>,
    user: Option<String>,
    #[ts(type = "number | null")]
    elapsed_seconds: Option<u64>,
    command: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "GpuVendorDto")]
pub(crate) enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "GpuStatusDto")]
pub(crate) enum GpuStatus {
    Ok,
    Detected,
    Unavailable,
}

pub(crate) fn sample_host_resources_with_progress<F>(
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
    on_snapshot: F,
) -> HostResourceBatchResult
where
    F: FnMut(&HostResourceSnapshot),
{
    let timeout = normalize_resource_sample_timeout_ms(timeout_ms);
    let checked_at = timestamp_now();
    let snapshots = sample_hosts_with_progress(host_aliases, timeout, sample_one_host, on_snapshot);

    HostResourceBatchResult {
        checked_at,
        snapshots,
    }
}

// 主机测试与资源采样共用 30 秒默认值及 1–60 秒范围。
pub(crate) fn normalize_resource_sample_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    let timeout = timeout_ms.unwrap_or(DEFAULT_RESOURCE_SAMPLE_TIMEOUT_MS);
    ssh::normalize_timeout_ms(Some(timeout)).min(MAX_RESOURCE_SAMPLE_TIMEOUT_MS)
}

fn sample_hosts_with_progress<S, F>(
    host_aliases: Vec<String>,
    timeout_ms: u64,
    sampler: S,
    mut on_snapshot: F,
) -> Vec<HostResourceSnapshot>
where
    S: Fn(String, u64) -> HostResourceSnapshot + Send + Sync + 'static,
    F: FnMut(&HostResourceSnapshot),
{
    let total = host_aliases.len();
    let mut ordered = vec![None; total];
    let mut pending = host_aliases.into_iter().enumerate();
    let sampler = Arc::new(sampler);
    let (sender, receiver) = mpsc::channel();

    // 固定并发池按完成顺序回传，并在空出槽位后立即补入下一台主机。
    for _ in 0..RESOURCE_SAMPLE_CONCURRENCY.min(total) {
        if let Some((index, host_alias)) = pending.next() {
            spawn_resource_sample(index, host_alias, timeout_ms, &sender, &sampler);
        }
    }

    for _ in 0..total {
        let (index, snapshot) = receiver
            .recv()
            .expect("resource sample workers always return one snapshot");
        on_snapshot(&snapshot);
        ordered[index] = Some(snapshot);
        if let Some((next_index, next_alias)) = pending.next() {
            spawn_resource_sample(next_index, next_alias, timeout_ms, &sender, &sampler);
        }
    }

    ordered
        .into_iter()
        .map(|snapshot| snapshot.expect("every resource sample slot is filled"))
        .collect()
}

fn spawn_resource_sample<S>(
    index: usize,
    host_alias: String,
    timeout_ms: u64,
    sender: &mpsc::Sender<(usize, HostResourceSnapshot)>,
    sampler: &Arc<S>,
) where
    S: Fn(String, u64) -> HostResourceSnapshot + Send + Sync + 'static,
{
    let sender = sender.clone();
    let sampler = Arc::clone(sampler);
    thread::spawn(move || {
        let fallback_alias = host_alias.clone();
        let snapshot = catch_unwind(AssertUnwindSafe(|| sampler(host_alias, timeout_ms)))
            .unwrap_or_else(|_| {
                failed_snapshot(
                    fallback_alias,
                    "Resource monitor worker failed.".into(),
                    None,
                )
            });
        let _ = sender.send((index, snapshot));
    });
}

fn sample_one_host(host_alias: String, timeout_ms: u64) -> HostResourceSnapshot {
    let alias = match ssh::validate_ssh_alias(&host_alias) {
        Ok(alias) => alias,
        Err(error) => return failed_snapshot(host_alias, error, None),
    };

    let output = match ssh::run_ssh_script(&alias, &resource_probe_script(), timeout_ms) {
        Ok(output) => output,
        Err(error) => return failed_snapshot(alias, error, None),
    };

    if !output.success() {
        return failed_snapshot_for_output(alias, &output);
    }

    parse_resource_stdout(&alias, &output.stdout, output.duration_ms)
}

fn failed_snapshot(
    host_alias: impl Into<String>,
    error: String,
    latency_ms: Option<u64>,
) -> HostResourceSnapshot {
    failed_snapshot_with_connectivity(
        host_alias,
        error,
        latency_ms,
        HostResourceSshStatus::Unknown,
        false,
    )
}

fn failed_snapshot_for_output(
    host_alias: impl Into<String>,
    output: &ssh::SshCommandOutput,
) -> HostResourceSnapshot {
    let connected = output
        .stdout
        .lines()
        .any(|line| line.trim() == "CH_SSH_CONNECTED=1");
    // 超时始终视为离线；已输出连接标记的远端脚本错误仍代表 SSH 可达。
    let ssh_status = if output.timed_out || !connected {
        HostResourceSshStatus::Offline
    } else {
        HostResourceSshStatus::Online
    };
    let latency_ms = (ssh_status == HostResourceSshStatus::Online).then_some(output.duration_ms);
    failed_snapshot_with_connectivity(
        host_alias,
        command_error(output),
        latency_ms,
        ssh_status,
        output.timed_out,
    )
}

fn failed_snapshot_with_connectivity(
    host_alias: impl Into<String>,
    error: String,
    latency_ms: Option<u64>,
    ssh_status: HostResourceSshStatus,
    timed_out: bool,
) -> HostResourceSnapshot {
    HostResourceSnapshot {
        host_alias: host_alias.into(),
        status: HostResourceStatus::Failed,
        ssh_status,
        timed_out,
        sampled_at: timestamp_now(),
        latency_ms,
        error: Some(error),
        cpu: None,
        memory: None,
        storage: None,
        gpu_tool: "none".into(),
        gpus: Vec::new(),
    }
}

fn parse_resource_stdout(host_alias: &str, stdout: &str, latency_ms: u64) -> HostResourceSnapshot {
    let sampled_at = timestamp_now();
    let mut cpu_first = None;
    let mut cpu_second = None;
    let mut load = None;
    let mut mem = None;
    let mut storage_rows = Vec::new();
    let mut cores = None;
    let mut model = None;
    let mut system_product = None;
    let mut gpu_tool = "none".to_string();
    let mut nvidia_rows = Vec::new();
    let mut nvidia_process_rows = Vec::new();
    let mut pci_rows = Vec::new();
    let mut amd_json = String::new();
    let mut in_amd_json = false;

    for line in stdout.lines() {
        if line == "CH_AMD_JSON_BEGIN" {
            in_amd_json = true;
            continue;
        }
        if line == "CH_AMD_JSON_END" {
            in_amd_json = false;
            continue;
        }
        if in_amd_json {
            amd_json.push_str(line);
            amd_json.push('\n');
            continue;
        }
        if let Some(value) = line.strip_prefix("CH_CPU_FIRST=") {
            cpu_first = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("CH_CPU_SECOND=") {
            cpu_second = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("CH_LOAD=") {
            load = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("CH_MEM=") {
            mem = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("CH_STORAGE_FS=") {
            storage_rows.push(value.to_string());
        } else if let Some(value) = line.strip_prefix("CH_CPU_CORES=") {
            cores = parse_u16(value);
        } else if let Some(value) = line.strip_prefix("CH_CPU_MODEL=") {
            model = nonempty(value);
        } else if let Some(value) = line.strip_prefix("CH_SYSTEM_PRODUCT=") {
            system_product = nonempty(value);
        } else if let Some(value) = line.strip_prefix("CH_GPU_TOOL=") {
            gpu_tool = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("CH_GPU_NVIDIA|") {
            nvidia_rows.push(value.to_string());
        } else if let Some(value) = line.strip_prefix("CH_GPU_PROCESS|") {
            nvidia_process_rows.push(value.to_string());
        } else if let Some(value) = line.strip_prefix("CH_GPU_PCI|") {
            pci_rows.push(value.to_string());
        }
    }

    let cpu = if cpu_first.is_some()
        || cpu_second.is_some()
        || load.is_some()
        || cores.is_some()
        || model.is_some()
    {
        Some(CpuSnapshot {
            usage_percent: cpu_usage(cpu_first.as_deref(), cpu_second.as_deref()),
            load1: load.as_deref().and_then(|value| parse_pipe_f64(value, 0)),
            load5: load.as_deref().and_then(|value| parse_pipe_f64(value, 1)),
            load15: load.as_deref().and_then(|value| parse_pipe_f64(value, 2)),
            cores,
            model,
        })
    } else {
        None
    };
    let memory = parse_memory(mem.as_deref());
    let storage = aggregate_storage_rows(&storage_rows, &sampled_at);

    // Prefer active vendor tools; PCI rows are a lightweight fallback for
    // hosts where GPU hardware exists but realtime tooling is unavailable.
    let mut gpus = if !nvidia_rows.is_empty() {
        let mut gpus = nvidia_rows
            .iter()
            .filter_map(|row| parse_nvidia_row(row))
            .collect::<Vec<_>>();
        let processes = nvidia_process_rows
            .iter()
            .filter_map(|row| parse_nvidia_process_row(row))
            .collect::<Vec<_>>();
        attach_nvidia_processes(&mut gpus, processes);
        gpus
    } else if !amd_json.trim().is_empty() {
        parse_amd_json(&amd_json)
    } else {
        pci_rows.iter().map(|row| gpu_from_pci(row)).collect()
    };

    let status = if cpu.is_some() && memory.is_some() && storage.is_some() {
        HostResourceStatus::Ok
    } else {
        HostResourceStatus::Partial
    };
    if gpus.is_empty() && gpu_tool != "none" && gpu_tool != "lspci" {
        gpus.push(GpuSnapshot {
            vendor: vendor_from_tool(&gpu_tool),
            index: None,
            uuid: None,
            name: format!("{gpu_tool} available, no GPU rows returned"),
            status: GpuStatus::Unavailable,
            memory_mode: GpuMemoryMode::Unknown,
            utilization_percent: None,
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_c: None,
            power_watts: None,
            driver_version: None,
            processes: Vec::new(),
        });
    }
    for gpu in &mut gpus {
        gpu.memory_mode = classify_gpu_memory_mode(gpu, system_product.as_deref());
    }

    HostResourceSnapshot {
        host_alias: host_alias.to_string(),
        status,
        ssh_status: HostResourceSshStatus::Online,
        timed_out: false,
        sampled_at,
        latency_ms: Some(latency_ms),
        error: None,
        cpu,
        memory,
        storage,
        gpu_tool,
        gpus,
    }
}

fn resource_probe_script() -> String {
    r#"printf 'CH_SSH_CONNECTED=1\n'
sanitize_monitor_field() {
  printf '%s' "$1" | tr '\r\n|' '   ' | cut -c 1-180
}
read_process_cpu_stat() {
  process_pid="$1"
  [ -n "$process_pid" ] && [ -r "/proc/$process_pid/stat" ] || return 0
  process_stat=$(sed -n '1p' "/proc/$process_pid/stat" 2>/dev/null) || return 0
  case "$process_stat" in
    *') '*) process_stat_tail=${process_stat##*) } ;;
    *) return 0 ;;
  esac
  printf '%s\n' "$process_stat_tail" | awk '
    NF >= 20 && $12 ~ /^[0-9]+$/ && $13 ~ /^[0-9]+$/ && $20 ~ /^[0-9]+$/ {
      printf "%.0f|%.0f\n", $20, $12 + $13
    }
  '
}
# Keep each uptime timestamp beside its /proc stat read so process count cannot skew the window.
read_process_cpu_sample() {
  process_pid="$1"
  process_stat=$(read_process_cpu_stat "$process_pid")
  process_uptime=$(awk '{ print $1; exit }' /proc/uptime 2>/dev/null)
  [ -n "$process_stat" ] && [ -n "$process_uptime" ] || return 0
  printf '%s|%s\n' "$process_stat" "$process_uptime"
}
process_cpu_percent() {
  process_pid="$1"
  first_stat=$(printf '%s\n' "$gpu_process_cpu_first" | awk -F'|' -v target="$process_pid" '$1 == target { print $2 "|" $3 "|" $4; exit }')
  second_stat=$(printf '%s\n' "$gpu_process_cpu_second" | awk -F'|' -v target="$process_pid" '$1 == target { print $2 "|" $3 "|" $4; exit }')
  [ -n "$first_stat" ] && [ -n "$second_stat" ] || return 0
  first_start=${first_stat%%|*}
  first_tail=${first_stat#*|}
  first_ticks=${first_tail%%|*}
  first_uptime=${first_tail#*|}
  second_start=${second_stat%%|*}
  second_tail=${second_stat#*|}
  second_ticks=${second_tail%%|*}
  second_uptime=${second_tail#*|}
  [ "$first_start" = "$second_start" ] || return 0
  # One saturated logical core is 100%; multithreaded processes may exceed 100%.
  awk -v first_ticks="$first_ticks" -v second_ticks="$second_ticks" \
      -v first_uptime="$first_uptime" -v second_uptime="$second_uptime" \
      -v clock_ticks="$monitor_clock_ticks" 'BEGIN {
    elapsed = second_uptime - first_uptime
    delta = second_ticks - first_ticks
    if (elapsed > 0 && delta >= 0 && clock_ticks > 0) {
      printf "%.1f", delta / clock_ticks / elapsed * 100
    }
  }'
}
cpu_line() {
  awk '/^cpu / { idle=$5+$6; total=0; for (i=2; i<=NF; i++) total += $i; printf "%s|%s\n", total, idle; exit }' /proc/stat 2>/dev/null
}
gpu_process_rows=''
gpu_process_cpu_first=''
gpu_process_cpu_second=''
monitor_clock_ticks=''
if command -v nvidia-smi >/dev/null 2>&1; then
  gpu_process_rows=$(nvidia-smi --query-compute-apps=gpu_uuid,pid,process_name,used_gpu_memory --format=csv,noheader,nounits 2>/dev/null)
  monitor_clock_ticks=$(getconf CLK_TCK 2>/dev/null || printf '')
  while IFS= read -r row; do
    [ -n "$row" ] || continue
    pid=$(printf '%s\n' "$row" | awk -F, '{ gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit }')
    process_sample=$(read_process_cpu_sample "$pid")
    if [ -n "$pid" ] && [ -n "$process_sample" ]; then
      gpu_process_cpu_first="${gpu_process_cpu_first}${pid}|${process_sample}
"
    fi
  done <<EOF
$gpu_process_rows
EOF
fi
first_cpu=$(cpu_line)
sleep 0.2 2>/dev/null || sleep 1
second_cpu=$(cpu_line)
if [ -n "$gpu_process_cpu_first" ]; then
  while IFS= read -r row; do
    [ -n "$row" ] || continue
    pid=$(printf '%s\n' "$row" | awk -F, '{ gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit }')
    process_sample=$(read_process_cpu_sample "$pid")
    if [ -n "$pid" ] && [ -n "$process_sample" ]; then
      gpu_process_cpu_second="${gpu_process_cpu_second}${pid}|${process_sample}
"
    fi
  done <<EOF
$gpu_process_rows
EOF
fi
printf 'CH_CPU_FIRST=%s\n' "$first_cpu"
printf 'CH_CPU_SECOND=%s\n' "$second_cpu"
awk '{ printf "CH_LOAD=%s|%s|%s\n", $1, $2, $3; exit }' /proc/loadavg 2>/dev/null
awk '/^MemTotal:/ { total=$2 } /^MemAvailable:/ { available=$2 } END { printf "CH_MEM=%s|%s\n", total+0, available+0 }' /proc/meminfo 2>/dev/null
__CH_STORAGE_PROBE__
printf 'CH_CPU_CORES=%s\n' "$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || printf '')"
awk -F: '/model name|Hardware|Processor/ { gsub(/^[ \t]+/, "", $2); print "CH_CPU_MODEL="$2; exit }' /proc/cpuinfo 2>/dev/null
system_product=''
if [ -r /sys/class/dmi/id/product_name ]; then
  system_product=$(sed -n '1p' /sys/class/dmi/id/product_name 2>/dev/null)
fi
printf 'CH_SYSTEM_PRODUCT=%s\n' "$(sanitize_monitor_field "$system_product")"
if command -v nvidia-smi >/dev/null 2>&1; then
  printf 'CH_GPU_TOOL=nvidia-smi\n'
  nvidia-smi --query-gpu=index,uuid,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,driver_version --format=csv,noheader,nounits 2>/dev/null | sed 's/^/CH_GPU_NVIDIA|/'
  printf '%s\n' "$gpu_process_rows" | while IFS= read -r row; do
    [ -n "$row" ] || continue
    pid=$(printf '%s\n' "$row" | awk -F, '{ gsub(/^[ \t]+|[ \t]+$/, "", $2); print $2; exit }')
    user=''
    elapsed=''
    cpu=''
    command=''
    if [ -n "$pid" ]; then
      user=$(ps -o user= -p "$pid" 2>/dev/null | awk 'NR==1 { gsub(/^[ \t]+|[ \t]+$/, ""); print; exit }')
      elapsed=$(ps -o etimes= -p "$pid" 2>/dev/null | awk 'NR==1 { gsub(/^[ \t]+|[ \t]+$/, ""); print; exit }')
      cpu=$(process_cpu_percent "$pid")
      if [ -r "/proc/$pid/cmdline" ]; then
        command=$(tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null)
      fi
      if [ -z "$command" ]; then
        command=$(ps -o args= -p "$pid" 2>/dev/null | awk 'NR==1 { gsub(/^[ \t]+|[ \t]+$/, ""); print; exit }')
      fi
    fi
    printf 'CH_GPU_PROCESS|%s|%s|%s|%s|%s\n' "$row" "$(sanitize_monitor_field "$user")" "$(sanitize_monitor_field "$elapsed")" "$(sanitize_monitor_field "$cpu")" "$(sanitize_monitor_field "$command")"
  done
elif command -v rocm-smi >/dev/null 2>&1; then
  printf 'CH_GPU_TOOL=rocm-smi\n'
  printf 'CH_AMD_JSON_BEGIN\n'
  rocm-smi --showuse --showmemuse --showtemp --showpower --json 2>/dev/null
  printf '\nCH_AMD_JSON_END\n'
elif command -v lspci >/dev/null 2>&1; then
  printf 'CH_GPU_TOOL=lspci\n'
  lspci 2>/dev/null | grep -Ei 'vga|3d|display' | sed 's/^/CH_GPU_PCI|/'
else
  printf 'CH_GPU_TOOL=none\n'
fi
"#
    .replace("__CH_STORAGE_PROBE__", storage_probe_script())
}

fn storage_probe_script() -> &'static str {
    // The deadline covers df and each identity lookup, keeping directory scans out of polling.
    r#"if command -v df >/dev/null 2>&1 && command -v stat >/dev/null 2>&1 && command -v timeout >/dev/null 2>&1; then
  storage_rows=$(LC_ALL=C timeout -k 1 2 sh -s 2>/dev/null <<'CH_STORAGE_PROBE'
df_rows=$(df -l -k -x tmpfs -x devtmpfs -x squashfs -x overlay -x ramfs -x rootfs -x autofs \
  --output=source,fstype,size,avail,target 2>/dev/null) || exit 1
printf '%s\n' "$df_rows" | awk '
  NR > 1 && $1 ~ /^\/dev\// && $1 !~ /^\/dev\/(loop|ram|zram)[0-9]/ &&
  ($2 !~ /^fuse/ || $2 == "fuseblk") && $3 ~ /^[0-9]+$/ && $4 ~ /^-?[0-9]+$/ {
    source=$1; filesystem=$2; total=$3; available=$4; mount=$0
    sub(/^[ \t]*[^ \t]+[ \t]+[^ \t]+[ \t]+[0-9]+[ \t]+-?[0-9]+[ \t]+/, "", mount)
    gsub(/[|\r\n]/, " ", source)
    printf "%s|%s|%s|%s|%s\n", total, available, source, filesystem, mount
  }' | while IFS='|' read -r total available source filesystem mount; do
  identity=$(stat -L -c '%d' -- "$mount" 2>/dev/null) || exit 1
  mount=$(printf '%s' "$mount" | tr '\r\n' '  ' | cut -c 1-180)
  printf 'CH_STORAGE_FS=%s|%s|%s|%s|%s|%s\n' "$identity" "$total" "$available" "$source" "$filesystem" "$mount"
done
CH_STORAGE_PROBE
  )
  if [ "$?" -eq 0 ] && [ -n "$storage_rows" ]; then
    printf '%s\n' "$storage_rows"
  fi
fi
"#
}

fn cpu_usage(first: Option<&str>, second: Option<&str>) -> Option<f64> {
    let (first_total, first_idle) = parse_cpu_line(first?)?;
    let (second_total, second_idle) = parse_cpu_line(second?)?;
    let total_delta = second_total.checked_sub(first_total)? as f64;
    let idle_delta = second_idle.checked_sub(first_idle)? as f64;
    if total_delta <= 0.0 {
        return None;
    }
    Some(round1(
        ((total_delta - idle_delta) / total_delta * 100.0).clamp(0.0, 100.0),
    ))
}

fn parse_cpu_line(value: &str) -> Option<(u64, u64)> {
    let mut parts = value.split('|');
    Some((parse_u64(parts.next()?)?, parse_u64(parts.next()?)?))
}

fn parse_memory(value: Option<&str>) -> Option<MemorySnapshot> {
    let value = value?;
    let total_kb = parse_pipe_u64(value, 0)?;
    let available_kb = parse_pipe_u64(value, 1)?;
    let total_bytes = total_kb.saturating_mul(1024);
    let available_bytes = available_kb.saturating_mul(1024);
    let used_percent = if total_bytes == 0 {
        None
    } else {
        Some(round1(
            ((total_bytes.saturating_sub(available_bytes)) as f64 / total_bytes as f64) * 100.0,
        ))
    };
    Some(MemorySnapshot {
        total_bytes: Some(total_bytes),
        available_bytes: Some(available_bytes),
        used_percent,
    })
}

fn parse_storage_row(value: &str) -> Option<(u64, StorageFilesystemSnapshot)> {
    let mut parts = value.splitn(6, '|');
    let identity = parts.next()?.trim().parse::<u64>().ok()?;
    let total_bytes = parts
        .next()?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)?;
    // df can report negative availability after reserved space is consumed.
    let available_kib = parts.next()?.trim().parse::<i128>().ok()?.max(0);
    let available_bytes = u64::try_from(available_kib).ok()?.checked_mul(1024)?;
    let source = parts.next()?.trim();
    let filesystem_type = parts.next()?.trim();
    let mount_point = parts.next()?.trim();
    if total_bytes == 0
        || available_bytes > total_bytes
        || !source.starts_with("/dev/")
        || filesystem_type.is_empty()
        || !mount_point.starts_with('/')
    {
        return None;
    }
    Some((
        identity,
        StorageFilesystemSnapshot {
            total_bytes,
            available_bytes,
            source: source.into(),
            filesystem_type: filesystem_type.into(),
            mount_points: vec![mount_point.into()],
        },
    ))
}

fn aggregate_storage_rows(rows: &[String], sampled_at: &str) -> Option<StorageSnapshot> {
    let mut filesystems: Vec<StorageFilesystemSnapshot> = Vec::new();
    let mut by_identity = HashMap::<u64, usize>::new();
    for row in rows {
        let (identity, candidate) = parse_storage_row(row)?;
        if let Some(&index) = by_identity.get(&identity) {
            // Bind mounts and aliases share capacity; count their kernel device identity once.
            let current = &mut filesystems[index];
            if current.total_bytes != candidate.total_bytes {
                return None;
            }
            current.available_bytes = current.available_bytes.min(candidate.available_bytes);
            for mount in candidate.mount_points {
                if !current.mount_points.contains(&mount) {
                    current.mount_points.push(mount);
                }
            }
        } else {
            by_identity.insert(identity, filesystems.len());
            filesystems.push(candidate);
        }
    }
    let total_bytes = filesystems.iter().try_fold(0u64, |sum, filesystem| {
        sum.checked_add(filesystem.total_bytes)
    })?;
    let available_bytes = filesystems.iter().try_fold(0u64, |sum, filesystem| {
        sum.checked_add(filesystem.available_bytes)
    })?;
    if total_bytes == 0 {
        return None;
    }
    Some(StorageSnapshot {
        total_bytes,
        available_bytes,
        // Reserved capacity is unavailable, so the meter agrees with available / total.
        used_percent: round1((total_bytes - available_bytes) as f64 / total_bytes as f64 * 100.0),
        filesystems,
        sampled_at: sampled_at.into(),
    })
}

fn parse_nvidia_row(row: &str) -> Option<GpuSnapshot> {
    let parts = row.split(',').map(|part| part.trim()).collect::<Vec<_>>();
    if parts.len() < 8 {
        return None;
    }
    let has_uuid = parts.len() >= 9;
    let name_index = if has_uuid { 2 } else { 1 };
    let metric_index = if has_uuid { 3 } else { 2 };
    Some(GpuSnapshot {
        vendor: GpuVendor::Nvidia,
        index: nonempty(parts[0]),
        uuid: if has_uuid { nonempty(parts[1]) } else { None },
        name: nonempty(parts[name_index]).unwrap_or_else(|| "NVIDIA GPU".into()),
        status: GpuStatus::Ok,
        memory_mode: GpuMemoryMode::Unknown,
        utilization_percent: parse_f64(parts[metric_index]).map(round1),
        memory_used_bytes: parse_mb_to_bytes(parts[metric_index + 1]),
        memory_total_bytes: parse_mb_to_bytes(parts[metric_index + 2]),
        temperature_c: parse_f64(parts[metric_index + 3]).map(round1),
        power_watts: parse_f64(parts[metric_index + 4]).map(round1),
        driver_version: nonempty(parts[metric_index + 5]),
        processes: Vec::new(),
    })
}

fn parse_nvidia_process_row(row: &str) -> Option<GpuProcessSnapshot> {
    let sections = row.splitn(5, '|').collect::<Vec<_>>();
    let csv = *sections.first()?;
    let user = sections.get(1).and_then(|value| nonempty(value));
    let (elapsed_seconds, cpu_usage_percent, command) = match sections.as_slice() {
        [_, _, elapsed, cpu, command] => (
            parse_u64(elapsed),
            parse_nonnegative_f64(cpu).map(round1),
            nonempty(command),
        ),
        [_, _, elapsed, command] => (parse_u64(elapsed), None, nonempty(command)),
        [_, _, command] => (None, None, nonempty(command)),
        _ => (None, None, None),
    };
    let parts = csv.split(',').map(|part| part.trim()).collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }
    Some(GpuProcessSnapshot {
        gpu_uuid: nonempty(parts[0]),
        pid: parse_u32(parts[1]),
        name: nonempty(parts[2]).unwrap_or_else(|| "GPU process".into()),
        cpu_usage_percent,
        used_memory_bytes: parse_mb_to_bytes(parts[3]),
        user,
        elapsed_seconds,
        command,
    })
}

fn attach_nvidia_processes(gpus: &mut [GpuSnapshot], processes: Vec<GpuProcessSnapshot>) {
    for process in processes {
        let target = process
            .gpu_uuid
            .as_deref()
            .and_then(|uuid| {
                gpus.iter()
                    .position(|gpu| gpu.uuid.as_deref() == Some(uuid))
            })
            .unwrap_or(0);
        if let Some(gpu) = gpus.get_mut(target) {
            gpu.processes.push(process);
        }
    }
}

fn parse_amd_json(content: &str) -> Vec<GpuSnapshot> {
    let Ok(JsonValue::Object(cards)) = serde_json::from_str::<JsonValue>(content) else {
        return Vec::new();
    };
    cards
        .iter()
        .filter_map(|(card, value)| {
            let object = value.as_object()?;
            let name = find_string(object, &[&["card", "series"], &["product", "name"]])
                .or_else(|| find_string(object, &[&["name"]]))
                .unwrap_or_else(|| card.clone());
            Some(GpuSnapshot {
                vendor: GpuVendor::Amd,
                index: card
                    .chars()
                    .filter(|ch| ch.is_ascii_digit())
                    .collect::<String>()
                    .into(),
                uuid: None,
                name,
                status: GpuStatus::Ok,
                memory_mode: GpuMemoryMode::Unknown,
                utilization_percent: find_number(object, &[&["gpu", "use"], &["gpu", "busy"]])
                    .map(round1),
                memory_used_bytes: None,
                memory_total_bytes: None,
                temperature_c: find_number(object, &[&["temperature"], &["temp"]]).map(round1),
                power_watts: find_number(object, &[&["power"]]).map(round1),
                driver_version: None,
                processes: Vec::new(),
            })
        })
        .collect()
}

fn gpu_from_pci(row: &str) -> GpuSnapshot {
    let lower = row.to_ascii_lowercase();
    let vendor = if lower.contains("nvidia") {
        GpuVendor::Nvidia
    } else if lower.contains("amd")
        || lower.contains("advanced micro devices")
        || lower.contains(" ati ")
        || lower.contains("ati technologies")
    {
        GpuVendor::Amd
    } else if lower.contains("intel") {
        GpuVendor::Intel
    } else {
        GpuVendor::Unknown
    };
    GpuSnapshot {
        vendor,
        index: None,
        uuid: None,
        name: row.trim().to_string(),
        status: GpuStatus::Detected,
        memory_mode: GpuMemoryMode::Unknown,
        utilization_percent: None,
        memory_used_bytes: None,
        memory_total_bytes: None,
        temperature_c: None,
        power_watts: None,
        driver_version: None,
        processes: Vec::new(),
    }
}

// DGX Spark exposes GB10 process allocations but reports dedicated FB memory as N/A.
// Keep other missing-capacity GPUs unknown so host RAM is never used as a generic fallback.
fn classify_gpu_memory_mode(gpu: &GpuSnapshot, system_product: Option<&str>) -> GpuMemoryMode {
    if gpu.memory_total_bytes.is_some_and(|value| value > 0) {
        return GpuMemoryMode::Dedicated;
    }
    if !matches!(gpu.vendor, GpuVendor::Nvidia)
        || !normalized_hardware_label(&gpu.name).contains("gb10")
    {
        return GpuMemoryMode::Unknown;
    }
    if system_product
        .is_some_and(|value| normalized_hardware_label(value).contains("nvidiadgxspark"))
    {
        GpuMemoryMode::Unified
    } else {
        GpuMemoryMode::Unknown
    }
}

fn normalized_hardware_label(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn find_number(
    object: &serde_json::Map<String, JsonValue>,
    keyword_sets: &[&[&str]],
) -> Option<f64> {
    object.iter().find_map(|(key, value)| {
        let lower = key.to_ascii_lowercase();
        if keyword_sets
            .iter()
            .any(|keywords| keywords.iter().all(|keyword| lower.contains(keyword)))
        {
            return json_number(value);
        }
        None
    })
}

fn find_string(
    object: &serde_json::Map<String, JsonValue>,
    keyword_sets: &[&[&str]],
) -> Option<String> {
    object.iter().find_map(|(key, value)| {
        let lower = key.to_ascii_lowercase();
        if keyword_sets
            .iter()
            .any(|keywords| keywords.iter().all(|keyword| lower.contains(keyword)))
        {
            return json_string(value);
        }
        None
    })
}

fn json_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(number) => number.as_f64(),
        JsonValue::String(text) => parse_f64(text),
        _ => None,
    }
}

fn json_string(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(text) => nonempty(text),
        JsonValue::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn command_error(output: &ssh::SshCommandOutput) -> String {
    if output.timed_out {
        return format!(
            "Resource sampling timed out after {} ms.",
            output.duration_ms
        );
    }
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }
    "Resource sampling failed.".into()
}

fn vendor_from_tool(tool: &str) -> GpuVendor {
    match tool {
        "nvidia-smi" => GpuVendor::Nvidia,
        "rocm-smi" => GpuVendor::Amd,
        _ => GpuVendor::Unknown,
    }
}

fn parse_pipe_f64(value: &str, index: usize) -> Option<f64> {
    value.split('|').nth(index).and_then(parse_f64)
}

fn parse_pipe_u64(value: &str, index: usize) -> Option<u64> {
    value.split('|').nth(index).and_then(parse_u64)
}

fn parse_mb_to_bytes(value: &str) -> Option<u64> {
    parse_f64(value).map(|mb| (mb * 1024.0 * 1024.0).round() as u64)
}

fn parse_f64(value: &str) -> Option<f64> {
    let cleaned = value.trim().trim_matches('"').replace('%', "");
    if cleaned.is_empty() || cleaned.eq_ignore_ascii_case("n/a") {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

fn parse_nonnegative_f64(value: &str) -> Option<f64> {
    let value = parse_f64(value)?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn parse_u64(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()
}

fn parse_u16(value: &str) -> Option<u16> {
    value.trim().parse::<u16>().ok()
}

fn parse_u32(value: &str) -> Option<u32> {
    value.trim().parse::<u32>().ok()
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("n/a") {
        None
    } else {
        Some(value.to_string())
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn timestamp_now() -> String {
    let local: DateTime<Local> = Local::now();
    local.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    #[test]
    fn sample_timeout_defaults_to_thirty_seconds_and_allows_sixty() {
        assert_eq!(normalize_resource_sample_timeout_ms(None), 30_000);
        assert_eq!(normalize_resource_sample_timeout_ms(Some(45_000)), 45_000);
        assert_eq!(normalize_resource_sample_timeout_ms(Some(60_000)), 60_000);
        assert_eq!(normalize_resource_sample_timeout_ms(Some(120_000)), 60_000);
        assert_eq!(normalize_resource_sample_timeout_ms(Some(0)), 1_000);
        assert_eq!(ssh::normalize_health_check_timeout_ms(Some(60_000)), 10_000);
    }

    #[test]
    fn parses_cpu_memory_and_nvidia_rows() {
        let output = "\
CH_CPU_FIRST=100|40
CH_CPU_SECOND=200|70
CH_LOAD=1.00|0.50|0.25
CH_MEM=1000|250
CH_STORAGE_FS=2049|1000|250|/dev/sda3|ext4|/
CH_CPU_CORES=8
CH_CPU_MODEL=AMD EPYC
CH_GPU_TOOL=nvidia-smi
CH_GPU_NVIDIA|0, GPU-aaaa, NVIDIA RTX 4090, 73, 1234, 24576, 66, 320.5, 550.54
CH_GPU_PROCESS|GPU-aaaa, 4242, python, 2048|amax|3661|412.7|python train.py
";
        let snapshot = parse_resource_stdout("gpu-host", output, 123);
        assert!(matches!(snapshot.status, HostResourceStatus::Ok));
        assert_eq!(
            snapshot.task_log_summary(),
            "Host gpu-host: resource sampling succeeded in 123 ms; 1 GPU(s) detected."
        );
        assert_eq!(snapshot.cpu.unwrap().usage_percent, Some(70.0));
        assert_eq!(snapshot.memory.unwrap().used_percent, Some(75.0));
        assert_eq!(snapshot.storage.unwrap().available_bytes, 256_000);
        assert_eq!(snapshot.gpus.len(), 1);
        assert_eq!(snapshot.gpus[0].uuid.as_deref(), Some("GPU-aaaa"));
        assert_eq!(snapshot.gpus[0].utilization_percent, Some(73.0));
        assert_eq!(snapshot.gpus[0].memory_mode, GpuMemoryMode::Dedicated);
        assert_eq!(snapshot.gpus[0].driver_version.as_deref(), Some("550.54"));
        assert_eq!(snapshot.gpus[0].processes.len(), 1);
        assert_eq!(snapshot.gpus[0].processes[0].pid, Some(4242));
        assert_eq!(
            snapshot.gpus[0].processes[0].used_memory_bytes,
            Some(2_147_483_648)
        );
        assert_eq!(snapshot.gpus[0].processes[0].user.as_deref(), Some("amax"));
        assert_eq!(snapshot.gpus[0].processes[0].elapsed_seconds, Some(3661));
        assert_eq!(snapshot.gpus[0].processes[0].cpu_usage_percent, Some(412.7));
    }

    #[test]
    fn aggregates_all_filesystems_and_counts_bind_mounts_once() {
        let output = "CH_STORAGE_FS=2049|1000|600|/dev/sda3|ext4|/\nCH_STORAGE_FS=64768|8000|800|/dev/mapper/home|xfs|/training data\nCH_STORAGE_FS=64768|8000|800|/dev/dm-0|xfs|/mirror\nCH_STORAGE_FS=2050|100|99|/dev/sda1|ext4|/boot\n";
        let snapshot = parse_resource_stdout("lab", output, 12);
        let storage = snapshot.storage.expect("aggregate storage");
        assert_eq!(storage.filesystems.len(), 3);
        assert_eq!(
            storage.filesystems[1].mount_points,
            ["/training data", "/mirror"]
        );
        assert_eq!(storage.total_bytes, 9_318_400);
        assert_eq!(storage.available_bytes, 1_534_976);
        assert_eq!(storage.used_percent, 83.5);
        assert_eq!(storage.sampled_at, snapshot.sampled_at);
    }

    #[test]
    fn rejects_invalid_storage_and_handles_exhausted_capacity() {
        for row in [
            "0|0|/dev/sda3|ext4|/",
            "100|101|/dev/sda3|ext4|/",
            "NaN|20|/dev/sda3|ext4|/",
            "100|NaN|/dev/sda3|ext4|/",
            "100|20",
            "100|20|/dev/sda3|ext4|relative",
            "18446744073709551615|1|/dev/sda3|ext4|/",
        ] {
            let full_row = format!("2049|{row}");
            assert!(parse_storage_row(&full_row).is_none(), "{row}");
        }
        for available in ["0", "-50"] {
            let row = format!("2049|100|{available}|/dev/sda3|ext4|/");
            let storage = aggregate_storage_rows(&[row], "sample-time").unwrap();
            assert_eq!(storage.available_bytes, 0);
            assert_eq!(storage.used_percent, 100.0);
        }
    }

    #[test]
    fn incomplete_or_overflowing_storage_never_claims_a_complete_total() {
        let valid = "2049|1000|250|/dev/sda3|ext4|/".to_string();
        assert!(aggregate_storage_rows(&[valid.clone(), "invalid".into()], "time").is_none());
        let changed = "2049|2000|500|/dev/sda3|ext4|/mirror".to_string();
        assert!(aggregate_storage_rows(&[valid, changed], "time").is_none());
        let huge = u64::MAX / 1024;
        let rows = [
            format!("2049|{huge}|0|/dev/sda3|ext4|/"),
            format!("2050|{huge}|0|/dev/sdb1|ext4|/home"),
        ];
        assert!(aggregate_storage_rows(&rows, "time").is_none());
    }

    #[test]
    fn missing_storage_keeps_other_metrics_and_ssh_online() {
        let output = "CH_CPU_FIRST=100|40\nCH_CPU_SECOND=200|70\nCH_MEM=1000|250\nCH_GPU_TOOL=nvidia-smi\nCH_GPU_NVIDIA|0, GPU-a, RTX 4090, 10, 100, 200, 40, 90, 550.54\n";
        let snapshot = parse_resource_stdout("lab", output, 12);
        assert!(matches!(snapshot.status, HostResourceStatus::Partial));
        assert!(matches!(snapshot.ssh_status, HostResourceSshStatus::Online));
        assert!(!snapshot.timed_out);
        assert!(snapshot.storage.is_none());
        assert_eq!(snapshot.cpu.unwrap().usage_percent, Some(70.0));
        assert_eq!(snapshot.memory.unwrap().used_percent, Some(75.0));
        assert_eq!(snapshot.gpus.len(), 1);
    }

    #[test]
    fn marks_dgx_spark_gb10_as_unified_memory() {
        let output = "\
CH_MEM=125511744|95643156
CH_SYSTEM_PRODUCT=NVIDIA_DGX_Spark
CH_GPU_TOOL=nvidia-smi
CH_GPU_NVIDIA|0, GPU-spark, NVIDIA GB10, 2, [N/A], [N/A], [N/A], 13, 580.95
CH_GPU_PROCESS|GPU-spark, 4242, python, 5067|wzy|1800|python train.py
";
        let snapshot = parse_resource_stdout("Spark-4", output, 80);
        let memory = snapshot.memory.as_ref().expect("host memory");
        assert_eq!(memory.total_bytes, Some(128_524_025_856));
        assert_eq!(snapshot.gpus[0].memory_mode, GpuMemoryMode::Unified);
        assert_eq!(snapshot.gpus[0].memory_total_bytes, None);
        assert_eq!(
            snapshot.gpus[0].processes[0].used_memory_bytes,
            Some(5_313_134_592)
        );
    }

    #[test]
    fn keeps_missing_gpu_capacity_unknown_on_other_hosts() {
        let output = "\
CH_MEM=125511744|95643156
CH_SYSTEM_PRODUCT=Other_Workstation
CH_GPU_TOOL=nvidia-smi
CH_GPU_NVIDIA|0, GPU-unknown, NVIDIA GB10, 2, [N/A], [N/A], [N/A], 13, 580.95
CH_GPU_PROCESS|GPU-unknown, 4242, python, 5067|wzy|1800|python train.py
";
        let snapshot = parse_resource_stdout("other", output, 80);
        assert_eq!(snapshot.gpus[0].memory_mode, GpuMemoryMode::Unknown);
    }

    #[test]
    fn keeps_gb10_unknown_without_dgx_spark_product_identity() {
        let output = "\
CH_MEM=125511744|95643156
CH_GPU_TOOL=nvidia-smi
CH_GPU_NVIDIA|0, GPU-unknown, NVIDIA GB10, 2, [N/A], [N/A], [N/A], 13, 580.95
";
        let snapshot = parse_resource_stdout("unknown", output, 80);
        assert_eq!(snapshot.gpus[0].memory_mode, GpuMemoryMode::Unknown);
    }

    #[test]
    fn keeps_legacy_nvidia_rows_without_uuid_compatible() {
        let gpu = parse_nvidia_row("0, NVIDIA RTX 4090, 73, 1234, 24576, 66, 320.5, 550.54")
            .expect("legacy row");
        assert_eq!(gpu.uuid, None);
        assert_eq!(gpu.name, "NVIDIA RTX 4090");
        assert_eq!(gpu.utilization_percent, Some(73.0));
        assert!(gpu.processes.is_empty());
    }

    #[test]
    fn attaches_nvidia_processes_by_uuid() {
        let mut gpus = vec![
            parse_nvidia_row("0, GPU-a, RTX 4090, 10, 100, 200, 40, 90, 550.54").unwrap(),
            parse_nvidia_row("1, GPU-b, RTX 4090, 20, 100, 200, 41, 91, 550.54").unwrap(),
        ];
        let processes = vec![
            parse_nvidia_process_row("GPU-b, 2002, python, 512|amax|90|400|python b.py").unwrap(),
            parse_nvidia_process_row("GPU-a, 1001, python, 256|root|120|python a.py").unwrap(),
        ];
        attach_nvidia_processes(&mut gpus, processes);
        assert_eq!(gpus[0].processes.len(), 1);
        assert_eq!(gpus[0].processes[0].pid, Some(1001));
        assert_eq!(gpus[1].processes.len(), 1);
        assert_eq!(gpus[1].processes[0].pid, Some(2002));
        assert_eq!(gpus[1].processes[0].elapsed_seconds, Some(90));
        assert_eq!(gpus[1].processes[0].cpu_usage_percent, Some(400.0));
        assert_eq!(gpus[0].processes[0].cpu_usage_percent, None);
    }

    #[test]
    fn parses_nvidia_process_rows_with_missing_optional_fields() {
        let process = parse_nvidia_process_row("GPU-a, 1001, [Not Found], N/A||||").unwrap();
        assert_eq!(process.gpu_uuid.as_deref(), Some("GPU-a"));
        assert_eq!(process.pid, Some(1001));
        assert_eq!(process.name, "[Not Found]");
        assert_eq!(process.used_memory_bytes, None);
        assert_eq!(process.user, None);
        assert_eq!(process.elapsed_seconds, None);
        assert_eq!(process.cpu_usage_percent, None);
        assert_eq!(process.command, None);
    }

    #[test]
    fn keeps_legacy_nvidia_process_rows_compatible() {
        let process = parse_nvidia_process_row("GPU-a, 1001, python, 256|root|python a.py")
            .expect("legacy process row");
        assert_eq!(process.user.as_deref(), Some("root"));
        assert_eq!(process.elapsed_seconds, None);
        assert_eq!(process.cpu_usage_percent, None);
        assert_eq!(process.command.as_deref(), Some("python a.py"));
    }

    #[test]
    fn process_cpu_percent_accepts_multicore_values_and_rejects_invalid_values() {
        let multicore =
            parse_nvidia_process_row("GPU-a, 1001, python, 256|root|120|437.86|python a.py")
                .unwrap();
        let negative =
            parse_nvidia_process_row("GPU-a, 1002, python, 256|root|120|-1|python b.py").unwrap();
        let not_finite =
            parse_nvidia_process_row("GPU-a, 1003, python, 256|root|120|NaN|python c.py").unwrap();

        assert_eq!(multicore.cpu_usage_percent, Some(437.9));
        assert_eq!(negative.cpu_usage_percent, None);
        assert_eq!(not_finite.cpu_usage_percent, None);
    }

    #[test]
    fn resource_probe_script_is_posix_and_keeps_per_core_process_cpu_semantics() {
        use std::io::Write as _;
        use std::process::Stdio;

        let script = resource_probe_script();
        assert!(script.contains("/proc/$process_pid/stat"));
        assert!(script.contains("read_process_cpu_sample"));
        assert!(script.contains("gpu_process_cpu_second"));
        assert!(script.contains("delta / clock_ticks / elapsed * 100"));

        let mut child = match std::process::Command::new("sh")
            .arg("-n")
            .stdin(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("could not start sh -n: {error}"),
        };
        child
            .stdin
            .take()
            .expect("sh stdin")
            .write_all(script.as_bytes())
            .expect("write resource probe script to sh");
        assert!(child.wait().expect("wait for sh -n").success());
    }

    #[test]
    fn parses_amd_json_rows() {
        let gpus = parse_amd_json(
            r#"{"card0":{"GPU use (%)":"12","Temperature (Sensor edge) (C)":"54.0","Average Graphics Package Power (W)":"80.5","Card series":"AMD Radeon Pro"}}"#,
        );
        assert_eq!(gpus.len(), 1);
        assert!(matches!(gpus[0].vendor, GpuVendor::Amd));
        assert_eq!(gpus[0].name, "AMD Radeon Pro");
        assert_eq!(gpus[0].utilization_percent, Some(12.0));
        assert_eq!(gpus[0].temperature_c, Some(54.0));
    }

    #[test]
    fn falls_back_to_pci_detection() {
        let gpu = gpu_from_pci("00:02.0 VGA compatible controller: Intel Corporation UHD Graphics");
        assert!(matches!(gpu.vendor, GpuVendor::Intel));
        assert!(matches!(gpu.status, GpuStatus::Detected));
        assert_eq!(gpu.utilization_percent, None);
    }

    #[test]
    fn marks_missing_cpu_or_memory_as_partial() {
        let snapshot = parse_resource_stdout("partial-host", "CH_GPU_TOOL=none\n", 42);
        assert!(matches!(snapshot.status, HostResourceStatus::Partial));
        assert!(matches!(snapshot.ssh_status, HostResourceSshStatus::Online));
        assert!(!snapshot.timed_out);
        assert_eq!(
            snapshot.task_log_summary(),
            "Host partial-host: resource sampling completed with partial data in 42 ms; 0 GPU(s) detected."
        );
        assert!(snapshot.cpu.is_none());
        assert!(snapshot.memory.is_none());
    }

    #[test]
    fn timed_out_error_includes_elapsed_duration() {
        let output = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            duration_ms: 30_042,
            timed_out: true,
        };

        assert_eq!(
            command_error(&output),
            "Resource sampling timed out after 30042 ms."
        );
        let snapshot = failed_snapshot_for_output("lab", &output);
        assert!(matches!(
            snapshot.ssh_status,
            HostResourceSshStatus::Offline
        ));
        assert!(snapshot.timed_out);
        assert_eq!(snapshot.latency_ms, None);
        assert_eq!(
            snapshot.task_log_summary(),
            "Host lab: resource sampling failed: Resource sampling timed out after 30042 ms."
        );
    }

    #[test]
    fn remote_script_failure_after_connection_keeps_ssh_online() {
        let output = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CH_SSH_CONNECTED=1\n".into(),
            stderr: "remote script failed".into(),
            exit_code: Some(1),
            duration_ms: 84,
            timed_out: false,
        };

        let snapshot = failed_snapshot_for_output("lab", &output);
        assert!(matches!(snapshot.ssh_status, HostResourceSshStatus::Online));
        assert!(!snapshot.timed_out);
        assert_eq!(snapshot.latency_ms, Some(84));
    }

    #[test]
    fn invalid_alias_returns_failed_snapshot_without_ssh() {
        let result =
            sample_host_resources_with_progress(vec!["bad alias!".into()], Some(3_000), |_| {});
        assert_eq!(result.snapshots.len(), 1);
        assert!(matches!(
            result.snapshots[0].status,
            HostResourceStatus::Failed
        ));
        assert_eq!(result.snapshots[0].host_alias, "bad alias!");
        assert!(matches!(
            result.snapshots[0].ssh_status,
            HostResourceSshStatus::Unknown
        ));
        assert!(!result.snapshots[0].timed_out);
        assert!(result.snapshots[0].error.is_some());
    }

    #[test]
    fn sliding_pool_reports_each_completed_host_and_preserves_result_order() {
        let aliases = vec!["slow", "fast-1", "fast-2", "later-1", "later-2"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let sampler_active = Arc::clone(&active);
        let sampler_max = Arc::clone(&max_active);
        let mut completion_order = Vec::new();

        let snapshots = sample_hosts_with_progress(
            aliases.clone(),
            3_000,
            move |alias, _| {
                let current = sampler_active.fetch_add(1, Ordering::SeqCst) + 1;
                sampler_max.fetch_max(current, Ordering::SeqCst);
                let delay_ms = if alias == "slow" { 160 } else { 12 };
                thread::sleep(Duration::from_millis(delay_ms));
                sampler_active.fetch_sub(1, Ordering::SeqCst);
                failed_snapshot(alias, "fixture".into(), None)
            },
            |snapshot| completion_order.push(snapshot.host_alias.clone()),
        );

        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.host_alias.as_str())
                .collect::<Vec<_>>(),
            aliases.iter().map(String::as_str).collect::<Vec<_>>()
        );
        assert_eq!(completion_order.len(), aliases.len());
        assert_eq!(completion_order.last().map(String::as_str), Some("slow"));
        assert!(
            completion_order.iter().position(|alias| alias == "later-1")
                < completion_order.iter().position(|alias| alias == "slow")
        );
        assert_eq!(
            max_active.load(Ordering::SeqCst),
            RESOURCE_SAMPLE_CONCURRENCY
        );
    }

    #[test]
    fn progress_event_serializes_request_and_completed_snapshot() {
        let event = HostResourceProgressEvent {
            request_id: "resource-request-1".into(),
            snapshot: failed_snapshot("alpha", "fixture".into(), None),
        };
        let json = serde_json::to_value(event).expect("serialize resource progress event");

        assert_eq!(json["requestId"], "resource-request-1");
        assert_eq!(json["snapshot"]["hostAlias"], "alpha");
        assert_eq!(json["snapshot"]["sshStatus"], "unknown");
        assert_eq!(json["snapshot"]["timedOut"], false);
    }
}
