mod adapters;
mod app_runtime;
#[cfg(test)]
mod backend_tests;
mod commands;
mod contracts;
mod domain;
mod hosts;
mod jobs;
mod platform;
mod profiles;
mod resource_monitor;
mod services;
mod settings;
mod skills;
mod ssh;
mod storage;
mod tasks;
mod updater;

pub use app_runtime::run;
pub(crate) use app_runtime::{app_display_name, hide_main_window};
pub(crate) use domain::*;

#[cfg(test)]
use adapters::is_missing_credential_error;
use adapters::{delete_profile_api_key_local, load_profile_api_key_local, profile_api_key_exists};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, Local, TimeZone};
use commands::*;
use contracts::{redact_error_text, TaskEvent};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use hosts::{load_hosts, save_current_hosts, save_hosts, save_hosts_state};
use profiles::{load_profiles, save_profiles};
use serde::{Deserialize, Serialize};
pub(crate) use services::updater_operations::{detect_network_proxy_status, run_blocking_command};
use services::*;
use settings::{read_settings, write_settings, AppSettings, CloseButtonBehavior, NetworkProxyMode};
use sha2::{Digest, Sha256};
use skills::{load_skills, managed_skills_dir, save_skills, skill_clone_cache_dir};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use storage::{AppPaths, TaskStore};
use tar::{Archive as TarArchive, Builder as TarBuilder};
use tasks::{TaskLog, TaskLogLevel, TaskRun, TaskStatus};
use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, Window, WindowEvent,
};
use tauri_plugin_updater::UpdaterExt;
use toml::map::Map as TomlMap;
use toml::Value as TomlValue;
use ts_rs::TS;
use updater::{AppUpdateState, AppUpdateStatus, GitHubReleaseResponse, StableUpdaterConfig};
use url::Url;
