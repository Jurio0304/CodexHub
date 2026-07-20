use crate::services::updater_operations::*;
use crate::tasks::{TaskStep, TaskStepStatus};
use crate::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized_fixture_source(source: &str) -> String {
        source.replace("\r\n", "\n")
    }

    fn test_profile(provider: &str) -> Profile {
        Profile {
            id: "profile-1".into(),
            name: "Profile One".into(),
            description: "Test profile".into(),
            model: "gpt-5-codex".into(),
            provider: provider.into(),
            base_url: if provider == "openai" {
                Some("https://proxy.example/v1".into())
            } else {
                Some("https://models.example/v1".into())
            },
            api_key_env_var: Some("CODEXHUB_TEST_API_KEY".into()),
            model_reasoning_effort: Some("medium".into()),
            plan_mode_reasoning_effort: Some("high".into()),
            fast_mode: true,
            service_tier: Some("auto".into()),
            approval_policy: "on-request".into(),
            sandbox_mode: "workspace-write".into(),
            extra_toml: String::new(),
            created_at: "1".into(),
            updated_at: "1".into(),
            source: "test".into(),
            credential_stored: true,
            host_ids: vec!["host-1".into()],
        }
    }

    fn test_host(alias: &str) -> Host {
        Host {
            id: format!("host-{alias}"),
            name: format!("Host {alias}"),
            host_alias: alias.into(),
            source: "test".into(),
            address: "127.0.0.1".into(),
            port: 22,
            username: "codex".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Unknown,
            os: String::new(),
            arch: String::new(),
            shell: String::new(),
            path: None,
            path_has_local_bin: None,
            codex_command_available: None,
            codex_installed: false,
            codex_version: String::new(),
            config_exists: None,
            api_config_name: None,
            api_config_source: None,
            api_key_env_var: None,
            api_key_env_present: None,
            skills_exists: None,
            skills_count: None,
            profile_id: None,
            skill_pack_ids: Vec::new(),
            tags: Vec::new(),
            last_seen: "never".into(),
            latency_ms: None,
        }
    }

    #[test]
    fn test_connection_rejects_missing_or_empty_alias_before_ssh() {
        let state = AppState::default();
        assert!(test_connection_host_alias(&state, "missing").is_err());

        state
            .hosts
            .lock()
            .expect("hosts mutex poisoned")
            .push(test_host("   "));
        assert!(test_connection_host_alias(&state, "host-   ").is_err());
    }

    #[test]
    fn stable_updater_status_is_pending_without_build_time_config() {
        let status = app_update_status_for_channel("stable", "0.2.0".into(), None, None);
        assert!(matches!(status.state, AppUpdateState::PendingConfiguration));
        assert_eq!(status.current_version, "0.2.0");
        assert!(!status.configured);
        assert!(!status.feed_configured);
        assert!(!status.signing_configured);
        assert!(status.message.contains(STABLE_UPDATE_ENDPOINT_ENV));
        assert!(status.message.contains(STABLE_UPDATER_PUBKEY_ENV));
    }

    #[test]
    fn up_to_date_update_status_reports_current_version_as_latest() {
        let config = StableUpdaterConfig {
            endpoint: Some("https://example.invalid/latest.json".into()),
            pubkey: Some("public-key".into()),
        };
        let status = app_update_status(
            "stable",
            "0.2.0",
            AppUpdateState::UpToDate,
            &config,
            None,
            Some("2026-07-03 15:05:35".into()),
            "CodexHub stable is up to date.".into(),
        );

        assert_eq!(status.latest_version.as_deref(), Some("0.2.0"));
        assert_eq!(status.checked_at.as_deref(), Some("2026-07-03 15:05:35"));
    }

    #[test]
    fn github_release_api_url_supports_latest_and_tagged_feeds() {
        let latest = Url::parse(
            "https://github.com/example-owner/CodexHub/releases/latest/download/latest.json",
        )
        .unwrap();
        let tagged = Url::parse(
            "https://github.com/example-owner/CodexHub/releases/download/v0.2.7/latest.json",
        )
        .unwrap();
        let asset_api =
            Url::parse("https://api.github.com/repos/example-owner/CodexHub/releases/assets/123")
                .unwrap();

        assert_eq!(
            github_release_api_url(&latest).as_deref(),
            Some("https://api.github.com/repos/example-owner/CodexHub/releases/latest")
        );
        assert_eq!(
            github_release_api_url(&tagged).as_deref(),
            Some("https://api.github.com/repos/example-owner/CodexHub/releases/tags/v0.2.7")
        );
        assert!(is_github_release_asset_api_endpoint(&asset_api));
    }

    #[test]
    fn dev_updater_status_is_disabled() {
        let status = app_update_status_for_channel("dev", "0.2.0".into(), None, None);
        assert!(matches!(status.state, AppUpdateState::Disabled));
        assert!(!status.configured);
        assert!(status
            .message
            .contains("Dev channel auto-updates are disabled"));
    }

    #[test]
    fn updater_pubkey_normalization_returns_tauri_pub_file_value() {
        let pubkey = "RWS19HRXxKw1q5/L9ZWqd5uQUpzxp8rDovvj1gMDY7gvZqhaBWrhAeVv";
        let pub_file =
            format!("untrusted comment: minisign public key: AB35ACC45774F4B5\n{pubkey}\n");
        let encoded_pub_file = general_purpose::STANDARD.encode(pub_file.as_bytes());

        assert_eq!(
            normalize_updater_pubkey(pubkey).as_deref(),
            Some(encoded_pub_file.as_str())
        );
        assert_eq!(
            normalize_updater_pubkey(&pub_file).as_deref(),
            Some(encoded_pub_file.as_str())
        );
        assert_eq!(
            normalize_updater_pubkey(&encoded_pub_file).as_deref(),
            Some(encoded_pub_file.as_str())
        );
    }

    #[test]
    fn legacy_app_settings_default_close_button_behavior_to_ask() {
        let settings: AppSettings = serde_json::from_str(
            r#"{
                "theme": "system",
                "fontPreset": "english",
                "platformAppearance": "auto",
                "setupGuideDismissed": true
            }"#,
        )
        .expect("legacy app settings deserialize");

        assert!(matches!(
            settings.close_button_behavior,
            CloseButtonBehavior::Ask
        ));
        assert!(matches!(
            settings.network_proxy_mode,
            NetworkProxyMode::Auto
        ));
        assert!(settings.network_proxy_url.is_empty());
        assert!(settings.resource_monitor_host_order.is_empty());
        assert!(settings.host_operation_log_popups);
        assert_eq!(
            serde_json::to_string(&CloseButtonBehavior::MinimizeToTray)
                .expect("serialize behavior"),
            "\"minimize-to-tray\""
        );
    }

    #[test]
    fn network_proxy_detection_redacts_manual_credentials() {
        let settings = AppSettings {
            network_proxy_mode: NetworkProxyMode::Manual,
            network_proxy_url: "http://user:secret@127.0.0.1:9".into(),
            ..Default::default()
        };
        let status = detect_network_proxy_status(&settings);
        let manual = status
            .candidates
            .iter()
            .find(|candidate| candidate.source == "manual")
            .expect("manual candidate");
        let url = manual.url.as_deref().expect("manual URL");

        assert!(url.contains("redacted"));
        assert!(!url.contains("secret"));
        assert_eq!(
            normalize_proxy_url("7890").expect("port proxy").to_string(),
            "http://127.0.0.1:7890/"
        );
    }

    fn empty_state() -> AppState {
        AppState::default()
    }

    #[test]
    fn profile_render_uses_builtin_openai_provider_without_custom_provider_table() {
        let toml = render_profile_toml(&test_profile("openai")).expect("render profile");

        assert!(toml.contains("model = \"gpt-5-codex\""));
        assert!(toml.contains("model_provider = \"openai\""));
        assert!(toml.contains("openai_base_url = \"https://proxy.example/v1\""));
        assert!(toml.contains("[features]"));
        assert!(toml.contains("fast_mode = true"));
        assert!(!toml.contains("[model_providers.openai]"));
    }

    #[test]
    fn profile_render_writes_custom_provider_table() {
        let toml = render_profile_toml(&test_profile("zhipu")).expect("render profile");

        assert!(toml.contains("model_provider = \"zhipu\""));
        assert!(toml.contains("[model_providers.zhipu]"));
        assert!(toml.contains("name = \"zhipu\""));
        assert!(toml.contains("base_url = \"https://models.example/v1\""));
        assert!(toml.contains("env_key = \"CODEXHUB_TEST_API_KEY\""));
    }

    #[test]
    fn profile_render_preserves_release_safe_settings_without_secret_values() {
        let mut profile = test_profile("openai");
        profile.service_tier = Some("flex".into());
        profile.approval_policy = "never".into();
        profile.sandbox_mode = "workspace-write".into();
        profile.extra_toml = "[history]\npersistence = \"save-all\"\n".into();

        let toml = render_profile_toml(&profile).expect("render profile");

        assert!(toml.contains("model_reasoning_effort = \"medium\""));
        assert!(toml.contains("plan_mode_reasoning_effort = \"high\""));
        assert!(toml.contains("service_tier = \"flex\""));
        assert!(toml.contains("approval_policy = \"never\""));
        assert!(toml.contains("sandbox_mode = \"workspace-write\""));
        assert!(toml.contains("[history]"));
        assert!(toml.contains("persistence = \"save-all\""));
        assert!(!toml.contains("credentialStored"));
        assert!(!toml.contains("api_key ="));
        assert!(!toml.contains("sk-"));
    }

    #[test]
    fn profile_extra_toml_rejects_structured_conflicts_and_merges_other_values() {
        let mut profile = test_profile("openai");
        profile.extra_toml =
            "[features]\nexperimental_resume = true\n[history]\npersistence = \"save-all\"\n"
                .into();
        let toml = render_profile_toml(&profile).expect("merge non-conflicting extra TOML");
        assert!(toml.contains("experimental_resume = true"));
        assert!(toml.contains("[history]"));

        profile.extra_toml = "model = \"other\"\n".into();
        assert!(render_profile_toml(&profile)
            .expect_err("model conflict")
            .contains("structured key `model`"));

        profile.extra_toml = "[features]\nfast_mode = false\n".into();
        assert!(render_profile_toml(&profile)
            .expect_err("features conflict")
            .contains("features.fast_mode"));

        profile.extra_toml = "[provider]\napi_key = \"not-for-disk\"\n".into();
        assert!(render_profile_toml(&profile)
            .expect_err("secret key")
            .contains("secret-like key `provider.api_key`"));
    }

    #[test]
    fn profile_export_and_render_do_not_include_key_material() {
        let profile = test_profile("zhipu");
        let rendered = render_profile_toml(&profile).expect("render profile");
        let exported = serde_json::to_string(&profile).expect("serialize profile");

        assert!(!rendered.contains("credential"));
        assert!(!rendered.contains("sk-"));
        assert!(!exported.contains("sk-"));
        assert!(!exported.contains("apiKeyValue"));
    }

    #[test]
    fn task_recorder_prepends_and_keeps_logs_redacted() {
        let state = empty_state();
        let fake_key = format!("{}{}", "sk-", "live12345678901234567890");
        let output = ssh::SshCommandOutput {
            command: "ssh lab echo ok".into(),
            stdout: ssh::redact_sensitive(&format!("token={fake_key}\nok")),
            stderr: ssh::redact_sensitive("password=super-secret-value"),
            exit_code: Some(0),
            duration_ms: 12,
            timed_out: false,
        };
        let older = skill_task(
            "task-old",
            "local",
            "Local machine",
            "Install skill",
            TaskStatus::Success,
            "Installed skill.",
            vec![command_log(
                "task-old",
                0,
                TaskLogLevel::Info,
                "install",
                &output,
            )],
        );
        let newer = skill_task(
            "task-new",
            "host-lab",
            "Host lab",
            "Apply profile",
            TaskStatus::Failed,
            "Profile apply failed.",
            vec![basic_log(
                "task-new",
                0,
                TaskLogLevel::Error,
                "remote config rejected",
            )],
        );

        record_task(&state, older).expect("persist older task");
        record_task(&state, newer).expect("persist newer task");
        let tasks = state.task_store.list(20).expect("list persisted tasks");
        let serialized = serde_json::to_string(&tasks).expect("serialize tasks");

        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["task-new", "task-old"]
        );
        assert_eq!(
            serde_json::to_string(&tasks[0].status).expect("serialize status"),
            "\"failed\""
        );
        assert!(serialized.contains("token=[redacted]"));
        assert!(serialized.contains("password=[redacted]"));
        assert!(!serialized.contains(&fake_key));
        assert!(!serialized.contains("super-secret-value"));
    }

    #[test]
    fn invalid_task_log_payload_fails_only_its_task_and_keeps_storage_healthy() {
        let state = empty_state();
        let mut running = skill_task(
            "task-invalid-log-payload",
            "host-lab",
            "Host lab",
            "Update Codex",
            TaskStatus::Running,
            "Update is running.",
            vec![basic_log(
                "task-invalid-log-payload",
                1,
                TaskLogLevel::Info,
                "Operation started.",
            )],
        );
        running.steps = vec![
            TaskStep {
                task_run_id: running.id.clone(),
                step_id: "finalization".into(),
                sequence: 0,
                status: TaskStepStatus::Running,
                summary: "Finalizing task.".into(),
                started_at: Some(timestamp_label()),
                ended_at: None,
            },
            TaskStep {
                task_run_id: running.id.clone(),
                step_id: "cleanup".into(),
                sequence: 1,
                status: TaskStepStatus::Pending,
                summary: "Waiting for cleanup.".into(),
                started_at: None,
                ended_at: None,
            },
        ];
        running.logs[0].step_id = Some("finalization".into());
        record_task(&state, running.clone()).expect("persist running task");

        let duplicate = basic_log(
            &running.id,
            2,
            TaskLogLevel::Info,
            "Duplicate payload fixture.",
        );
        running.status = TaskStatus::Success;
        running.ended_at = Some(timestamp_label());
        running.logs.push(duplicate.clone());
        running.logs.push(duplicate);

        let error = record_task(&state, running).expect_err("invalid payload must fail");
        assert!(error.contains("Task payload invariant violation"));
        assert!(state
            .task_storage_error
            .lock()
            .expect("task storage health lock")
            .is_none());
        ensure_task_storage_healthy(&state).expect("unrelated durable work remains available");

        let persisted = state
            .task_store
            .get("task-invalid-log-payload")
            .expect("read failed task")
            .expect("failed task remains visible");
        assert!(matches!(persisted.status, TaskStatus::Failed));
        assert!(persisted.ended_at.is_some());
        assert!(matches!(persisted.steps[0].status, TaskStepStatus::Failed));
        assert!(matches!(persisted.steps[1].status, TaskStepStatus::Skipped));
    }

    #[test]
    fn genuine_task_store_failure_still_blocks_later_durable_operations() {
        let state = AppState::new(TaskStore::unavailable("injected database failure".into()));
        let task = skill_task(
            "task-unavailable-store",
            "local",
            "Local",
            "Write durable task",
            TaskStatus::Failed,
            "Write failed.",
            vec![basic_log(
                "task-unavailable-store",
                1,
                TaskLogLevel::Error,
                "Write failed.",
            )],
        );

        let error = record_task(&state, task).expect_err("unavailable store must fail");
        assert!(error.contains("Persistent task storage failed"));
        assert!(ensure_task_storage_healthy(&state).is_err());
    }

    #[test]
    fn profile_and_local_skill_tasks_use_expected_public_shapes() {
        let host = test_host("lab");
        let reload = RemoteCodexReloadResult {
            mode: RemoteCodexReloadMode::None,
            status: RemoteCodexReloadStatus::NotRequested,
            targeted_count: 0,
            stopped_count: 0,
            preserved_cli_count: 0,
            replacement_observed: false,
            message: "Remote Codex reload was not requested.".into(),
        };
        let profile_task = profile_apply_task(
            "task-profile-1",
            &host,
            TaskStatus::Success,
            "Applied profile.",
            vec![basic_log("task-profile-1", 0, TaskLogLevel::Info, "done")],
            TaskStepStatus::Success,
            &reload,
        );
        let skill_task = local_skill_task("Install skill", "Installed local skill.", true);

        assert_eq!(profile_task.action, "Apply profile");
        assert_eq!(profile_task.host_id, "host-lab");
        assert_eq!(
            serde_json::to_string(&profile_task.status).expect("serialize profile status"),
            "\"success\""
        );
        assert_eq!(profile_task.steps.len(), 2);
        assert_eq!(profile_task.steps[0].step_id, "profile-apply");
        assert_eq!(profile_task.steps[1].step_id, "remote-codex-reload");
        assert_eq!(skill_task.host_id, "local");
        assert_eq!(skill_task.host_name, "Local machine");
        assert_eq!(skill_task.action, "Install skill");
    }

    #[test]
    fn cc_switch_sqlite_profiles_read_codex_providers_without_secrets() {
        let dir = env::temp_dir().join(format!("codexhub-cc-switch-{}", timestamp_millis()));
        fs::create_dir_all(&dir).expect("create temp cc-switch dir");
        let db_path = dir.join("cc-switch.db");
        fs::write(
            dir.join("settings.json"),
            r#"{"currentProviderCodex":"codex-current"}"#,
        )
        .expect("write settings");

        {
            let connection = rusqlite::Connection::open(&db_path).expect("open sqlite fixture");
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE providers (
                        id TEXT NOT NULL,
                        app_type TEXT NOT NULL,
                        name TEXT NOT NULL,
                        settings_config TEXT NOT NULL,
                        website_url TEXT,
                        category TEXT,
                        created_at INTEGER,
                        sort_index INTEGER,
                        notes TEXT,
                        icon TEXT,
                        icon_color TEXT,
                        meta TEXT NOT NULL DEFAULT '{}',
                        is_current BOOLEAN NOT NULL DEFAULT 0,
                        in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
                        PRIMARY KEY (id, app_type)
                    );
                    CREATE TABLE provider_endpoints (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        provider_id TEXT NOT NULL,
                        app_type TEXT NOT NULL,
                        url TEXT NOT NULL,
                        added_at INTEGER
                    );
                    "#,
                )
                .expect("create cc-switch schema");
            let current_config = "model = \"gpt-5.5\"\nmodel_provider = \"custom\"\nmodel_reasoning_effort = \"xhigh\"\n[features]\nfast_mode = true\n[model_providers.custom]\nname = \"custom\"\n";
            let other_config = "model = \"gpt-5-codex\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://config.example/v1\"\nenv_key = \"REMOTE_API_KEY\"\n";
            let current_settings = serde_json::json!({
                "auth": { "OPENAI_API_KEY": "sk-test-secret", "auth_mode": "api_key" },
                "config": current_config
            })
            .to_string();
            let other_settings = serde_json::json!({ "config": other_config }).to_string();
            let claude_settings =
                serde_json::json!({ "config": "model = \"claude\"\n" }).to_string();
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current) VALUES (?1, 'codex', ?2, ?3, NULL, 'custom', 0)",
                    rusqlite::params!["codex-current", "Current Codex", current_settings],
                )
                .expect("insert current codex");
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current) VALUES (?1, 'codex', ?2, ?3, NULL, 'custom', 0)",
                    rusqlite::params!["codex-other", "Other Codex", other_settings],
                )
                .expect("insert other codex");
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current) VALUES (?1, 'claude', ?2, ?3, NULL, 'custom', 0)",
                    rusqlite::params!["claude-provider", "Claude", claude_settings],
                )
                .expect("insert non-codex");
            connection
                .execute(
                    "INSERT INTO provider_endpoints (provider_id, app_type, url, added_at) VALUES (?1, 'codex', ?2, 1)",
                    rusqlite::params!["codex-current", "https://endpoint.example/v1"],
                )
                .expect("insert endpoint");
        }

        let profiles = parse_cc_switch_sqlite_profiles(&db_path).expect("parse sqlite profiles");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].profile.name, "Current Codex");
        assert_eq!(
            profiles[0].profile.base_url.as_deref(),
            Some("https://endpoint.example/v1")
        );
        assert_eq!(profiles[0].profile.provider, "custom");
        assert_eq!(
            profiles[0].profile.model_reasoning_effort.as_deref(),
            Some("xhigh")
        );
        assert!(profiles[0].profile.fast_mode);
        assert_eq!(profiles[0].api_key.as_deref(), Some("sk-test-secret"));
        assert_eq!(
            profiles[1].profile.api_key_env_var.as_deref(),
            Some("REMOTE_API_KEY")
        );
        let serialized = serde_json::to_string(&profiles[0].profile).expect("serialize profile");
        assert!(!serialized.contains("sk-test-secret"));
        assert!(profiles
            .iter()
            .all(|record| !record.profile.credential_stored));

        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_file(dir.join("settings.json"));
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn keyring_missing_entry_message_is_not_a_hard_failure() {
        assert!(is_missing_credential_error(
            "No matching entry found in secure storage"
        ));
    }

    #[test]
    fn cc_switch_raw_db_recovery_extracts_config_without_auth() {
        let settings = serde_json::json!({
            "auth": { "api_key": "sk-raw-secret" },
            "config": "model = \"gpt-5.5\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://raw.example/v1\"\n"
        })
        .to_string();
        let content = format!(
            "noise 891d8cb1-69b8-4cac-9368-4944b1ec1735codexRaw Provider{settings}https://fallback.example/v1custom"
        );
        let profiles = parse_cc_switch_raw_db_profiles(&content, Path::new("cc-switch.db"));

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].profile.name, "Raw Provider");
        assert_eq!(
            profiles[0].profile.base_url.as_deref(),
            Some("https://raw.example/v1")
        );
        assert_eq!(
            profiles[0].profile.api_key_env_var.as_deref(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(profiles[0].api_key.as_deref(), Some("sk-raw-secret"));
        let serialized = serde_json::to_string(&profiles[0].profile).expect("serialize profile");
        assert!(!serialized.contains("sk-raw-secret"));
    }

    #[test]
    fn profile_apply_script_checks_no_change_before_backup() {
        let script = profile_apply_commit_script(
            "/tmp/codexhub-profile-test.toml",
            "abc123",
            42,
            "{\"profileId\":\"profile-1\"}",
            "12345",
        );
        let cmp_index = script
            .find("cmp -s \"$config\" \"$staged\"")
            .expect("cmp guard");
        let backup_index = script
            .find("cp -p \"$config\" \"$backup\"")
            .expect("backup command");

        assert!(cmp_index < backup_index);
        assert!(script.contains("CODEXHUB_PROFILE_BACKUP"));
        assert!(script.contains("CODEXHUB_PROFILE_VALIDATION"));
        assert!(!script.contains("CODEXHUB_RELOAD"));
        assert!(!script.contains("kill -TERM"));
    }

    #[test]
    fn profile_apply_metadata_contains_expected_identity() {
        let metadata = AppliedProfileMetadata {
            profile_id: "profile-1".into(),
            profile_name: "Profile One".into(),
            applied_at: "12345".into(),
            codexhub_version: "0.1.0".into(),
        };
        let json = serde_json::to_string(&metadata).expect("serialize metadata");

        assert!(json.contains("\"profileId\":\"profile-1\""));
        assert!(json.contains("\"profileName\":\"Profile One\""));
        assert!(json.contains("\"codexhubVersion\":\"0.1.0\""));
    }

    #[test]
    fn npm_latest_metadata_parser_extracts_dist_tag_latest() {
        let metadata = r#"{
          "dist-tags": { "latest": "0.142.2", "beta": "0.143.0-beta.1" }
        }"#;

        let latest = parse_npm_latest_metadata(metadata).expect("parse npm latest");

        assert_eq!(latest, "0.142.2");
    }

    #[test]
    fn npm_latest_metadata_parser_rejects_html_missing_and_unsafe_values() {
        assert!(parse_npm_latest_metadata("<html>login</html>").is_err());
        assert!(parse_npm_latest_metadata(r#"{"dist-tags":{}}"#).is_err());
        assert!(parse_npm_latest_metadata(r#"{"dist-tags":{"latest":"0.1.0;rm -rf /"}}"#).is_err());
    }

    #[test]
    fn latest_codex_cache_refreshes_after_daily_four_am_boundary() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T05:00:00+08:00").expect("now");
        let fresh = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: Some("2026-06-28T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };
        let stale = LatestCodexVersion {
            checked_at: Some("2026-06-28T03:59:00+08:00".into()),
            ..fresh.clone()
        };

        assert!(latest_codex_cache_is_fresh(&fresh, now));
        assert!(!latest_codex_cache_is_fresh(&stale, now));
    }

    #[test]
    fn latest_codex_cache_uses_previous_day_boundary_before_four_am() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T03:00:00+08:00").expect("now");
        let fresh = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: Some("2026-06-27T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };
        let stale = LatestCodexVersion {
            checked_at: Some("2026-06-27T03:59:00+08:00".into()),
            ..fresh.clone()
        };

        assert!(latest_codex_cache_is_fresh(&fresh, now));
        assert!(!latest_codex_cache_is_fresh(&stale, now));
    }

    #[test]
    fn latest_codex_cache_requires_version_and_checked_at() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T05:00:00+08:00").expect("now");
        let missing_version = LatestCodexVersion {
            version: None,
            checked_at: Some("2026-06-28T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };
        let missing_checked_at = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: None,
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };

        assert!(!latest_codex_cache_is_fresh(&missing_version, now));
        assert!(!latest_codex_cache_is_fresh(&missing_checked_at, now));
    }

    #[test]
    fn latest_codex_fetch_failure_returns_cached_version_or_error_only_result() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T05:00:00+08:00").expect("now");
        let cache = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: Some("2026-06-28T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };

        let stale = latest_codex_result_from_fetch(Err("offline".into()), Some(cache.clone()), now);
        let empty = latest_codex_result_from_fetch(Err("offline".into()), None, now);

        assert_eq!(stale.version, cache.version);
        assert_eq!(stale.checked_at, cache.checked_at);
        assert_eq!(stale.error.as_deref(), Some("offline"));
        assert_eq!(empty.version, None);
        assert_eq!(empty.checked_at, None);
        assert_eq!(empty.error.as_deref(), Some("offline"));
    }

    #[test]
    fn skill_metadata_parser_reads_frontmatter_and_falls_back_to_directory_name() {
        let content =
            "---\nname: \"Example Skill\"\ndescription: Run example workflow\nversion: '0.4.2'\n---\nBody";
        let parsed =
            parse_skill_metadata(content, Path::new("example-skill")).expect("parse skill");

        assert_eq!(parsed.name, "Example Skill");
        assert_eq!(parsed.description.as_deref(), Some("Run example workflow"));
        assert_eq!(parsed.version.as_deref(), Some("0.4.2"));

        let fallback = parse_skill_metadata("# Instructions", Path::new("draft-helper"))
            .expect("fallback skill");
        assert_eq!(fallback.name, "draft-helper");
        let description_only = parse_skill_metadata(
            "---\ndescription: Description only\n---\nBody",
            Path::new("helper"),
        )
        .expect("description-only skill");
        assert_eq!(description_only.name, "helper");
        assert_eq!(
            description_only.description.as_deref(),
            Some("Description only")
        );
        assert!(parse_skill_metadata("", Path::new("empty")).is_err());
    }

    #[test]
    fn skill_ids_and_remote_names_reject_unsafe_values() {
        assert_eq!(
            safe_skill_id("Example Skill++").expect("slug"),
            "example-skill"
        );
        assert_eq!(
            safe_skill_id("owner/repo").expect("github slug"),
            "owner-repo"
        );
        assert!(safe_skill_id("!!!").is_err());

        assert_eq!(
            validate_remote_skill_dir_name("Paper_Review-1.2").expect("remote name"),
            "Paper_Review-1.2"
        );
        assert!(validate_remote_skill_dir_name("../secret").is_err());
        assert!(validate_remote_skill_dir_name("paper review").is_err());
        assert!(validate_remote_skill_dir_name(".").is_err());
    }

    #[test]
    fn skill_candidate_scan_uses_root_or_immediate_children() {
        let root = env::temp_dir().join(format!("codexhub-skill-scan-{}", timestamp_millis()));
        let child_a = root.join("example-skill");
        let child_b = root.join("no-skill");
        let nested = root.join("nested").join("deep-skill");
        fs::create_dir_all(&child_a).expect("create child skill");
        fs::create_dir_all(&child_b).expect("create child without skill");
        fs::create_dir_all(&nested).expect("create nested skill");
        fs::write(child_a.join("SKILL.md"), "# Paper").expect("write child skill");
        fs::write(nested.join("SKILL.md"), "# Deep").expect("write nested skill");

        let candidates = skill_candidate_dirs(&root).expect("scan children");
        assert_eq!(candidates, vec![child_a.clone()]);

        fs::write(root.join("SKILL.md"), "# Root").expect("write root skill");
        let root_candidates = skill_candidate_dirs(&root).expect("scan root");
        assert_eq!(root_candidates, vec![root.clone()]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn installed_skill_scan_includes_second_level_system_skills() {
        let root = env::temp_dir().join(format!(
            "codexhub-installed-skill-scan-{}",
            timestamp_millis()
        ));
        let local = root.join("pdf");
        let system = root.join(".system").join("imagegen");
        let too_deep = root.join("nested").join("deep").join("ignored");
        fs::create_dir_all(&local).expect("create local skill");
        fs::create_dir_all(&system).expect("create system skill");
        fs::create_dir_all(&too_deep).expect("create deep skill");
        fs::write(local.join("SKILL.md"), "# PDF").expect("write local skill");
        fs::write(system.join("SKILL.md"), "# Image").expect("write system skill");
        fs::write(too_deep.join("SKILL.md"), "# Deep").expect("write deep skill");

        let candidates = installed_skill_candidate_dirs(&root).expect("scan installed skills");

        assert!(candidates.contains(&local));
        assert!(candidates.contains(&system));
        assert!(!candidates.contains(&too_deep));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn github_download_url_parser_is_strict_and_supports_tree_paths() {
        assert!(is_allowed_github_repo_url(
            "https://github.com/owner/example-skill"
        ));
        assert!(is_allowed_github_repo_url(
            "https://github.com/owner/example-skill.git"
        ));
        assert!(is_allowed_github_repo_url(
            "https://github.com/openai/skills/tree/main/skills/.curated/winui-app"
        ));
        assert!(!is_allowed_github_repo_url(
            "git@github.com:owner/example-skill.git"
        ));
        assert!(!is_allowed_github_repo_url("https://github.com/owner"));
        assert!(!is_allowed_github_repo_url(
            "https://github.com/owner/example/extra"
        ));
        assert!(!is_allowed_github_repo_url(
            "https://github.com/openai/skills/tree/main/../secret"
        ));
        assert!(!is_allowed_github_repo_url(
            "https://github.com/openai/skills/tree/main/skills//bad"
        ));
        let repo_url = parse_github_skill_url("https://github.com/owner/example-skill.git")
            .expect("parse repo url");
        assert_eq!(repo_url.owner, "owner");
        assert_eq!(repo_url.repo, "example-skill");
        let tree_url = parse_github_skill_url(
            "https://github.com/openai/skills/tree/main/skills/.curated/winui-app",
        )
        .expect("parse tree url");
        assert_eq!(tree_url.owner, "openai");
        assert_eq!(tree_url.repo, "skills");
        assert_eq!(tree_url.clone_url, "https://github.com/openai/skills.git");
        assert_eq!(tree_url.branch.as_deref(), Some("main"));
        assert_eq!(
            tree_url.skill_subpath.as_deref(),
            Some(Path::new("skills/.curated/winui-app"))
        );
    }

    #[test]
    fn ensure_child_path_allows_children_and_rejects_siblings() {
        let root = env::temp_dir().join(format!("codexhub-child-path-{}", timestamp_millis()));
        let child = root.join("managed").join("example-skill");
        let sibling = root
            .parent()
            .expect("temp root parent")
            .join(format!("codexhub-sibling-{}", timestamp_millis()));
        fs::create_dir_all(&child).expect("create child");
        fs::create_dir_all(&sibling).expect("create sibling");

        assert!(ensure_child_path(&root, &child).is_ok());
        assert!(ensure_child_path(&root, &sibling).is_err());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&sibling);
    }

    #[test]
    fn remote_project_skill_paths_require_absolute_or_home_roots() {
        let (home_expr, home_display) =
            remote_skill_root(&RemoteSkillScope::Project, Some("~/work/repo"))
                .expect("home project path");
        assert_eq!(home_expr, "$HOME/'work/repo'/.codex/skills");
        assert_eq!(home_display, "~/work/repo/.codex/skills");

        let (absolute_expr, absolute_display) =
            remote_skill_root(&RemoteSkillScope::Project, Some("/srv/repo"))
                .expect("absolute project path");
        assert_eq!(absolute_expr, "'/srv/repo'/.codex/skills");
        assert_eq!(absolute_display, "/srv/repo/.codex/skills");

        assert!(remote_skill_root(&RemoteSkillScope::Project, Some("relative/repo")).is_err());
        assert!(remote_skill_root(&RemoteSkillScope::Project, Some("~/")).is_err());
        assert!(remote_skill_root(&RemoteSkillScope::Project, Some("/srv/repo\nbad")).is_err());
    }

    #[test]
    fn remote_skill_list_parser_extracts_validity_and_paths() {
        let stdout = "CODEXHUB_SKILL_ROOT=/home/test/.codex/skills\n\
CODEXHUB_REMOTE_SKILL\texample-skill\tyes\tvalid\t/home/test/.codex/skills/example-skill\tRun example workflow\n\
CODEXHUB_REMOTE_SKILL\tdraft-helper\tno\tmissing-skill-md\t/home/test/.codex/skills/draft-helper\t\n";

        let skills = parse_remote_skill_list(stdout);

        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "example-skill");
        assert_eq!(skills[0].description, "Run example workflow");
        assert!(skills[0].has_skill_md);
        assert_eq!(skills[1].status, "missing-skill-md");
        assert!(!skills[1].has_skill_md);
    }

    #[test]
    fn remote_skill_list_parser_accepts_space_delimited_output() {
        let stdout = "CODEXHUB_SKILL_ROOT=/home/jy/.codex/skills\n\
CODEXHUB_REMOTE_SKILL imagegen yes valid /home/jy/.codex/skills/.system/imagegen Generate or edit raster images\n\
CODEXHUB_REMOTE_SKILL openai-docs yes valid /home/jy/.codex/skills/.system/openai-docs\n\
CODEXHUB_SKILL_ROOT=/home/jy/.codex/superpowers/skills\n\
CODEXHUB_REMOTE_SKILL brainstorming yes valid /home/jy/.codex/superpowers/skills/brainstorming\n\
CODEXHUB_SKILL_COUNT=3\n";

        let skills = parse_remote_skill_list(stdout);

        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].name, "imagegen");
        assert_eq!(skills[0].description, "Generate or edit raster images");
        assert_eq!(
            skills[2].path,
            "/home/jy/.codex/superpowers/skills/brainstorming"
        );
        assert!(skills.iter().all(|skill| skill.has_skill_md));
    }

    #[test]
    fn remote_skill_list_parser_deduplicates_paths() {
        let stdout = "CODEXHUB_REMOTE_SKILL\timagegen\tyes\tvalid\t/home/test/.codex/skills/.system/imagegen\n\
CODEXHUB_REMOTE_SKILL\timagegen\tyes\tvalid\t/home/test/.codex/skills/.system/imagegen\n";

        let skills = parse_remote_skill_list(stdout);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "imagegen");
    }

    #[test]
    fn remote_skill_list_script_scans_hidden_second_level_skills() {
        let script = remote_skill_list_script();

        assert!(script.contains("\"$root\"/.[!.]*"));
        assert!(script.contains("\"$root\"/..?*"));
        assert!(script.contains("\"$dir\"/.[!.]*"));
        assert!(script.contains("\"$dir\"/..?*"));
        assert!(script.contains("$HOME/.codex/superpowers/skills"));
        assert!(script.contains("scan_child_dir"));
        assert!(script.contains("scan_root \"$HOME/.codex/skills\""));
        assert!(script.contains("scan_root \"$HOME/.codex/superpowers/skills\""));
        assert!(script.contains("emit_skill_dir \"$nested\""));
        assert!(script.contains("extract_skill_description"));
        assert!(script.contains("description=$(extract_skill_description \"$dir\")"));
        assert!(script.contains("CODEXHUB_REMOTE_SKILL\\t%s\\t%s\\t%s\\t%s\\t%s"));
        assert!(script.contains("scan_find_fallback"));
        assert!(script.contains("find \"$root\" -mindepth 1 -maxdepth 3 -type f -name SKILL.md"));
        assert!(!script.contains("roots=\""));
    }

    #[test]
    fn remote_skill_count_script_matches_hidden_second_level_scan() {
        let script = remote_skill_count_script();

        assert!(script.contains("\"$root\"/.[!.]*"));
        assert!(script.contains("\"$root\"/..?*"));
        assert!(script.contains("\"$dir\"/.[!.]*"));
        assert!(script.contains("\"$dir\"/..?*"));
        assert!(script.contains("$HOME/.codex/superpowers/skills"));
        assert!(script.contains("scan_root \"$HOME/.codex/skills\""));
        assert!(script.contains("scan_root \"$HOME/.codex/superpowers/skills\""));
        assert!(script.contains("count_skill_dir \"$nested\""));
        assert!(!script.contains("roots=\""));
        assert!(!script.contains("find \"$HOME/.codex/skills\" -mindepth 1 -maxdepth 1"));
    }

    #[test]
    fn remote_skill_install_scripts_encode_conflict_policies_and_safety_guards() {
        let backup = remote_skill_install_script(
            "/tmp/skill.tgz",
            "$HOME/.codex/skills",
            "example-skill",
            &SkillConflictPolicy::Backup,
            "12345",
        );
        assert!(backup.contains("policy='backup'"));
        assert!(backup.contains("tar is required on the remote host"));
        assert!(backup.contains("grep -Eq '(^|/)\\.\\.(/|$)|^/'"));
        assert!(backup.contains("mv \"$target\" \"$backup\""));
        assert!(backup.contains("CODEXHUB_SKILL_BACKUP"));
        assert!(!backup.contains("sudo "));

        let skip = remote_skill_install_script(
            "/tmp/skill.tgz",
            "$HOME/.codex/skills",
            "example-skill",
            &SkillConflictPolicy::Skip,
            "12345",
        );
        assert!(skip.contains("policy='skip'"));
        assert!(skip.contains("skipped=yes"));

        let overwrite = remote_skill_install_script(
            "/tmp/skill.tgz",
            "$HOME/.codex/skills",
            "example-skill",
            &SkillConflictPolicy::Overwrite,
            "12345",
        );
        assert!(overwrite.contains("policy='overwrite'"));
        assert!(overwrite.contains("rm -rf \"$target\""));
    }

    #[test]
    fn remote_skill_delete_script_hard_deletes_after_directory_check() {
        let script = remote_skill_delete_script("$HOME/.codex/skills", "example-skill", "12345");

        assert!(script.contains("rm -rf \"$target\""));
        assert!(script.contains("CODEXHUB_SKILL_COUNT"));
        assert!(!script.contains("codexhub.deleted.$timestamp"));
        assert!(!script.contains("mv \"$target\" \"$backup\""));
        assert!(!script.contains("sudo "));
    }

    #[test]
    fn installed_skill_download_script_packages_exact_cached_path() {
        let script = remote_installed_skill_archive_script(
            "/home/me/.codex/superpowers/skills/.system/example-skill",
            "/tmp/codexhub-skill-download-test.tgz",
        );

        assert!(
            script.contains("target='/home/me/.codex/superpowers/skills/.system/example-skill'")
        );
        assert!(script.contains("tar -czf \"$archive\" -C \"$parent\" \"$base\""));
        assert!(script.contains("CODEXHUB_SKILL_ARCHIVE"));
        assert!(!script.contains("sudo "));
    }

    #[test]
    fn installed_skill_delete_script_hard_deletes_exact_cached_path() {
        let script = remote_installed_skill_delete_script("/home/me/.codex/skills/example-skill");

        assert!(script.contains("target='/home/me/.codex/skills/example-skill'"));
        assert!(script.contains("rm -rf \"$target\""));
        assert!(script.contains("CODEXHUB_SKILL_COUNT"));
        assert!(!script.contains("codexhub.deleted"));
        assert!(!script.contains("sudo "));
    }

    #[test]
    fn profile_apply_host_link_moves_between_profiles_and_dedupes_alias() {
        let mut profiles = vec![
            Profile {
                id: "old-profile".into(),
                name: "Old".into(),
                host_ids: vec!["lab-alias".into(), "other-host".into()],
                ..test_profile("openai")
            },
            Profile {
                id: "new-profile".into(),
                name: "New".into(),
                host_ids: vec!["host-42".into()],
                ..test_profile("openai")
            },
        ];

        sync_profile_host_ids(&mut profiles, "new-profile", "host-42", "lab-alias");
        sync_profile_host_ids(&mut profiles, "new-profile", "host-42", "LAB-ALIAS");

        assert_eq!(profiles[0].host_ids, vec!["other-host"]);
        assert_eq!(profiles[1].host_ids, vec!["host-42"]);
    }

    #[test]
    fn probe_unknown_config_clears_profile_host_link() {
        let mut profiles = vec![
            Profile {
                id: "profile-1".into(),
                name: "Known".into(),
                host_ids: vec!["host-42".into(), "lab-alias".into(), "other-host".into()],
                ..test_profile("openai")
            },
            Profile {
                id: "profile-2".into(),
                name: "Other".into(),
                host_ids: vec!["LAB-ALIAS".into()],
                ..test_profile("openai")
            },
        ];

        clear_profile_host_ids(&mut profiles, "host-42", "lab-alias");

        assert_eq!(profiles[0].host_ids, vec!["other-host"]);
        assert!(profiles[1].host_ids.is_empty());
    }

    #[cfg(target_os = "linux")]
    fn isolated_reload_fixture_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "codexhub-reload-{name}-{}-{}",
            std::process::id(),
            timestamp_millis()
        ));
        fs::create_dir_all(&root).expect("create isolated reload fixture root");
        root
    }

    #[cfg(target_os = "linux")]
    fn write_fake_proc_dir(proc_dir: &Path, pid: u32, starttime: u64, comm: &str, argv: &[&str]) {
        use std::os::unix::fs::PermissionsExt as _;

        fs::create_dir_all(proc_dir).expect("create fake proc directory");
        let uid = String::from_utf8(
            std::process::Command::new("id")
                .arg("-u")
                .output()
                .expect("read test uid")
                .stdout,
        )
        .expect("uid is UTF-8");
        let uid = uid.trim();
        fs::write(
            proc_dir.join("status"),
            format!("Name:\t{comm}\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\n"),
        )
        .expect("write fake status");
        fs::write(proc_dir.join("comm"), format!("{comm}\n")).expect("write fake comm");
        let mut cmdline = Vec::new();
        for arg in argv {
            cmdline.extend_from_slice(arg.as_bytes());
            cmdline.push(0);
        }
        fs::write(proc_dir.join("cmdline"), cmdline).expect("write fake cmdline");
        fs::write(
            proc_dir.join("stat"),
            format!("{pid} ({comm}) S {} {starttime}\n", vec!["0"; 18].join(" ")),
        )
        .expect("write fake stat");
        fs::set_permissions(proc_dir, fs::Permissions::from_mode(0o700))
            .expect("protect fake proc directory");
    }

    #[cfg(target_os = "linux")]
    fn write_fake_proc(root: &Path, pid: u32, starttime: u64, comm: &str, argv: &[&str]) {
        write_fake_proc_dir(&root.join(pid.to_string()), pid, starttime, comm, argv);
    }

    #[cfg(target_os = "linux")]
    fn run_isolated_reload_fixture(
        root: &Path,
        mode: RemoteCodexReloadMode,
    ) -> (RemoteCodexReloadResult, Vec<u32>) {
        use std::io::Write as _;
        use std::process::Stdio;

        let root_text = root.to_string_lossy();
        assert!(!root_text.contains('\''));
        let generated = remote_codex_reload_script(mode)
            .replace("/proc", root_text.as_ref())
            .replace("kill -TERM \"$pid\"", "codexhub_test_term \"$pid\"")
            .replace("sleep 1", "codexhub_test_sleep");
        assert!(!generated.contains("kill -TERM"));
        let harness = format!(
            r#"test_root='{root_text}'
codexhub_test_term() {{
  pid=$1
  printf '%s\n' "$pid" >>"$test_root/term-calls"
  [ -f "$test_root/.term-fail-$pid" ] && return 1
  [ -f "$test_root/.term-keep-$pid" ] && return 0
  rm -rf "$test_root/$pid"
  if [ -d "$test_root/.reuse-$pid" ]; then
    mv "$test_root/.reuse-$pid" "$test_root/$pid"
  fi
  if [ -d "$test_root/.spawn-replacement" ]; then
    mv "$test_root/.spawn-replacement" "$test_root/900"
  fi
  return 0
}}
codexhub_test_sleep() {{ :; }}
{generated}
"#
        );
        let mut child = std::process::Command::new("sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start isolated reload shell");
        child
            .stdin
            .take()
            .expect("isolated reload stdin")
            .write_all(harness.as_bytes())
            .expect("write isolated reload harness");
        let output = child
            .wait_with_output()
            .expect("wait for isolated reload shell");
        let command_output = ssh::SshCommandOutput {
            command: "isolated reload fixture".into(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
            duration_ms: 0,
            timed_out: false,
        };
        let result = parse_remote_codex_reload_result(mode, &command_output);
        let term_calls = fs::read_to_string(root.join("term-calls"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.parse::<u32>().ok())
            .collect();
        (result, term_calls)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn remote_codex_reload_script_executes_against_isolated_proc_fixtures() {
        let app_root = isolated_reload_fixture_root("app-services");
        write_fake_proc(
            &app_root,
            101,
            1_001,
            "codex",
            &["/home/test/.local/bin/codex", "app-server"],
        );
        write_fake_proc(
            &app_root,
            102,
            1_002,
            "codex-remote-co",
            &["/home/test/.local/bin/codex-remote-control"],
        );
        write_fake_proc(
            &app_root,
            103,
            1_003,
            "codex",
            &["/home/test/.local/bin/codex", "exec"],
        );
        write_fake_proc_dir(
            &app_root.join(".spawn-replacement"),
            900,
            9_000,
            "codex",
            &["/home/test/.local/bin/codex", "app-server"],
        );
        let (app_result, app_terms) =
            run_isolated_reload_fixture(&app_root, RemoteCodexReloadMode::AppServices);
        assert_eq!(app_result.status, RemoteCodexReloadStatus::Reconnected);
        assert_eq!(app_result.targeted_count, 2);
        assert_eq!(app_result.stopped_count, 2);
        assert_eq!(app_result.preserved_cli_count, 1);
        assert_eq!(app_terms, vec![101, 102]);

        let all_root = isolated_reload_fixture_root("all-codex");
        write_fake_proc(
            &all_root,
            201,
            2_001,
            "codex",
            &["/home/test/.local/bin/codex", "exec"],
        );
        write_fake_proc(
            &all_root,
            202,
            2_002,
            "codex",
            &["/home/test/.local/bin/codex", "resume"],
        );
        let (all_result, all_terms) =
            run_isolated_reload_fixture(&all_root, RemoteCodexReloadMode::AllCodex);
        assert_eq!(all_result.status, RemoteCodexReloadStatus::Reloaded);
        assert_eq!(all_result.targeted_count, 2);
        assert_eq!(all_result.stopped_count, 2);
        assert_eq!(all_terms, vec![201, 202]);

        let zero_root = isolated_reload_fixture_root("zero");
        let (zero_result, zero_terms) =
            run_isolated_reload_fixture(&zero_root, RemoteCodexReloadMode::AppServices);
        assert_eq!(zero_result.status, RemoteCodexReloadStatus::NotRunning);
        assert!(zero_terms.is_empty());

        let term_failure_root = isolated_reload_fixture_root("term-failure");
        write_fake_proc(
            &term_failure_root,
            301,
            3_001,
            "codex",
            &["/home/test/.local/bin/codex", "app-server"],
        );
        fs::write(term_failure_root.join(".term-fail-301"), "fail").expect("mark TERM failure");
        let (term_failure, term_failure_calls) =
            run_isolated_reload_fixture(&term_failure_root, RemoteCodexReloadMode::AppServices);
        assert_eq!(term_failure.status, RemoteCodexReloadStatus::ManualRequired);
        assert!(term_failure.message.contains("TERM request failed"));
        assert_eq!(term_failure_calls, vec![301]);

        let exit_timeout_root = isolated_reload_fixture_root("exit-timeout");
        write_fake_proc(
            &exit_timeout_root,
            401,
            4_001,
            "codex",
            &["/home/test/.local/bin/codex", "login"],
        );
        fs::write(exit_timeout_root.join(".term-keep-401"), "keep")
            .expect("mark process as still running");
        let (exit_timeout, exit_timeout_calls) =
            run_isolated_reload_fixture(&exit_timeout_root, RemoteCodexReloadMode::AllCodex);
        assert_eq!(exit_timeout.status, RemoteCodexReloadStatus::ManualRequired);
        assert!(exit_timeout.message.contains("closing the remaining"));
        assert_eq!(exit_timeout_calls, vec![401]);

        let reused_root = isolated_reload_fixture_root("pid-reuse");
        write_fake_proc(
            &reused_root,
            501,
            5_001,
            "codex",
            &["/home/test/.local/bin/codex", "app-server"],
        );
        write_fake_proc_dir(
            &reused_root.join(".reuse-501"),
            501,
            5_999,
            "codex",
            &["/home/test/.local/bin/codex", "app-server"],
        );
        let (reused, reused_calls) =
            run_isolated_reload_fixture(&reused_root, RemoteCodexReloadMode::AppServices);
        assert_eq!(reused.status, RemoteCodexReloadStatus::Reconnected);
        assert!(reused.replacement_observed);
        assert_eq!(reused_calls, vec![501]);

        let uncertain_root = isolated_reload_fixture_root("identity-uncertain");
        write_fake_proc(
            &uncertain_root,
            601,
            6_001,
            "codex",
            &["/home/test/.local/bin/codex", "app-server"],
        );
        fs::remove_file(uncertain_root.join("601/cmdline"))
            .expect("make process identity incomplete");
        let (uncertain, uncertain_calls) =
            run_isolated_reload_fixture(&uncertain_root, RemoteCodexReloadMode::AppServices);
        assert_eq!(uncertain.status, RemoteCodexReloadStatus::ManualRequired);
        assert!(uncertain.message.contains("could not be verified safely"));
        assert!(uncertain_calls.is_empty());

        for root in [
            app_root,
            all_root,
            zero_root,
            term_failure_root,
            exit_timeout_root,
            reused_root,
            uncertain_root,
        ] {
            assert!(root.starts_with(env::temp_dir()));
            assert!(root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("codexhub-reload-")));
            fs::remove_dir_all(root).expect("remove isolated reload fixture root");
        }
    }

    #[test]
    fn remote_codex_reload_script_uses_strict_proc_identity_and_graceful_term() {
        let app_services = remote_codex_reload_script(RemoteCodexReloadMode::AppServices);
        let all_codex = remote_codex_reload_script(RemoteCodexReloadMode::AllCodex);

        assert!(app_services.contains("mode='app-services'"));
        assert!(all_codex.contains("mode='all-codex'"));
        for token in [
            "for proc_dir in /proc/[0-9]*",
            "umask 077",
            "proc_uid=$(awk '/^Uid:/",
            "proc_start=$(sed 's/^[^)]*) //'",
            "codex:codex:app-server",
            "codex:codex:remote-control",
            "codex-app-server:codex-app-server:*",
            "codex-remote-control:codex-remote-control:*",
            "[ \"$proc_base\" = \"codex\" ] && [ \"$proc_comm\" = \"codex\" ]",
            "kill -TERM \"$pid\"",
            "kill_failed=$((kill_failed + 1))",
            "identity_uncertain=yes",
            "manual-required unverified-process",
            "command -v grep",
            "preserved_cli=$((preserved_cli + 1))",
            "[ \"$proc_start\" = \"$expected_start\" ]",
            "[ \"$elapsed\" -lt 5 ]",
            "[ \"$elapsed\" -lt 15 ]",
            "CODEXHUB_RELOAD_PRESERVED_CLI",
            "CODEXHUB_RELOAD_REPLACEMENT_OBSERVED",
        ] {
            assert!(
                app_services.contains(token),
                "missing reload safety token: {token}"
            );
        }
        for forbidden in [
            "pkill",
            "killall",
            "SIGKILL",
            "kill -KILL",
            "kill -9",
            "kill -- -",
            "kill -TERM -",
            "kill -TERM 0",
            "[[",
            "]]",
            "<(",
            ">(",
            "declare -a",
            "function ",
        ] {
            assert!(!app_services.contains(forbidden));
            assert!(!all_codex.contains(forbidden));
        }
        assert!(!app_services.contains("kill -TERM \"$pid\" 2>/dev/null || true"));
        assert!(
            app_services
                .matches("[ \"$proc_start\" = \"$expected_start\" ]")
                .count()
                >= 3
        );
        assert!(app_services.contains("printf '%s %s %s\\n' \"$pid\" \"$proc_start\" \"$kind\""));
        assert!(app_services.contains("grep -q \"^$pid $proc_start \" \"$candidates\""));
        for line in app_services.lines().filter(|line| line.contains("printf")) {
            assert!(!line.contains("proc_argv"));
            assert!(!line.contains("proc_arg1"));
            assert!(!line.contains("proc_arg2"));
        }
    }

    #[test]
    fn remote_codex_reload_script_is_posix_shell_syntax() {
        use std::io::Write as _;
        use std::process::Stdio;

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
            .write_all(remote_codex_reload_script(RemoteCodexReloadMode::AppServices).as_bytes())
            .expect("write reload script to sh");
        assert!(child.wait().expect("wait for sh -n").success());

        for mode in [RemoteCodexReloadMode::None, RemoteCodexReloadMode::AllCodex] {
            let status = std::process::Command::new("sh")
                .arg("-n")
                .stdin(Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    child
                        .stdin
                        .take()
                        .expect("sh stdin")
                        .write_all(remote_codex_reload_script(mode).as_bytes())?;
                    child.wait()
                })
                .expect("validate reload script with sh -n");
            assert!(status.success());
        }
    }

    #[test]
    fn remote_codex_reload_result_requires_replacement_for_app_reconnect() {
        let reconnected = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=reconnected\nCODEXHUB_RELOAD_TARGETED=2\nCODEXHUB_RELOAD_STOPPED=2\nCODEXHUB_RELOAD_PRESERVED_CLI=1\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=yes\nCODEXHUB_RELOAD_REASON=replacement-observed".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 100,
            timed_out: false,
        };
        let parsed =
            parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &reconnected);
        assert_eq!(parsed.status, RemoteCodexReloadStatus::Reconnected);
        assert_eq!(parsed.targeted_count, 2);
        assert_eq!(parsed.stopped_count, 2);
        assert_eq!(parsed.preserved_cli_count, 1);
        assert!(parsed.replacement_observed);

        let not_running = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=not-running\nCODEXHUB_RELOAD_TARGETED=0\nCODEXHUB_RELOAD_STOPPED=0\nCODEXHUB_RELOAD_PRESERVED_CLI=2\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=no-matching-process".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 20,
            timed_out: false,
        };
        let parsed =
            parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &not_running);
        assert_eq!(parsed.status, RemoteCodexReloadStatus::NotRunning);
        assert_eq!(parsed.targeted_count, 0);
        assert_eq!(parsed.preserved_cli_count, 2);

        let all_codex_reloaded = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=reloaded\nCODEXHUB_RELOAD_TARGETED=3\nCODEXHUB_RELOAD_STOPPED=3\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=processes-stopped".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 120,
            timed_out: false,
        };
        let parsed =
            parse_remote_codex_reload_result(RemoteCodexReloadMode::AllCodex, &all_codex_reloaded);
        assert_eq!(parsed.status, RemoteCodexReloadStatus::Reloaded);
        assert_eq!(parsed.targeted_count, 3);
        assert_eq!(parsed.stopped_count, 3);

        let no_replacement = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=manual-required\nCODEXHUB_RELOAD_TARGETED=1\nCODEXHUB_RELOAD_STOPPED=1\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=replacement-not-observed".into(),
            stderr: String::new(),
            exit_code: Some(4),
            duration_ms: 15_000,
            timed_out: false,
        };
        let parsed =
            parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &no_replacement);
        assert_eq!(parsed.status, RemoteCodexReloadStatus::ManualRequired);
        assert!(parsed.message.contains("15 seconds"));

        let term_failed = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=manual-required\nCODEXHUB_RELOAD_TARGETED=2\nCODEXHUB_RELOAD_STOPPED=2\nCODEXHUB_RELOAD_PRESERVED_CLI=1\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=term-failed".into(),
            stderr: String::new(),
            exit_code: Some(4),
            duration_ms: 100,
            timed_out: false,
        };
        let parsed =
            parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &term_failed);
        assert_eq!(parsed.status, RemoteCodexReloadStatus::ManualRequired);
        assert!(parsed.message.contains("TERM request failed"));
        assert_eq!(parsed.stopped_count, 2);
        assert_eq!(parsed.preserved_cli_count, 1);

        for (reason, targeted, stopped, expected_message) in [
            ("old-process-still-running", 2, 1, "closing the remaining"),
            ("unverified-process", 0, 0, "could not be verified safely"),
            (
                "proc-unavailable",
                0,
                0,
                "identity could not be verified safely",
            ),
        ] {
            let output = ssh::SshCommandOutput {
                command: "ssh lab sh -s".into(),
                stdout: format!(
                    "CODEXHUB_RELOAD_STATUS=manual-required\nCODEXHUB_RELOAD_TARGETED={targeted}\nCODEXHUB_RELOAD_STOPPED={stopped}\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON={reason}"
                ),
                stderr: String::new(),
                exit_code: Some(4),
                duration_ms: 5_000,
                timed_out: false,
            };
            let parsed =
                parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &output);
            assert_eq!(parsed.status, RemoteCodexReloadStatus::ManualRequired);
            assert!(parsed.message.contains(expected_message));
        }

        let timed_out = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=manual-required\nCODEXHUB_RELOAD_TARGETED=1\nCODEXHUB_RELOAD_STOPPED=0\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=old-process-still-running".into(),
            stderr: "timed out".into(),
            exit_code: None,
            duration_ms: 30_000,
            timed_out: true,
        };
        let parsed =
            parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &timed_out);
        assert_eq!(parsed.status, RemoteCodexReloadStatus::Failed);

        let noisy = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=not-running\nCODEXHUB_RELOAD_TARGETED=0\nCODEXHUB_RELOAD_STOPPED=0\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=no-matching-process\ncodex app-server --api-key sk-test-secret".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 20,
            timed_out: false,
        };
        let parsed = parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &noisy);
        let serialized = serde_json::to_string(&parsed).expect("serialize reload result");
        assert!(!serialized.contains("codex app-server"));
        assert!(!serialized.contains("sk-test-secret"));
        let log = basic_log(
            "task-reload-noisy",
            0,
            TaskLogLevel::Info,
            &remote_codex_reload_log_message(&parsed),
        );
        let serialized_log = serde_json::to_string(&log).expect("serialize reload task log");
        assert!(serialized_log.contains("targeted=0"));
        assert!(serialized_log.contains("status=not-running"));
        assert!(!serialized_log.contains("codex app-server"));
        assert!(!serialized_log.contains("sk-test-secret"));
        assert!(log.command.is_none());
        assert!(log.stdout.is_none());
        assert!(log.stderr.is_none());

        let malformed = ssh::SshCommandOutput {
            command: "ssh lab sh -s".into(),
            stdout: "CODEXHUB_RELOAD_STATUS=reconnected".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 10,
            timed_out: false,
        };
        let parsed =
            parse_remote_codex_reload_result(RemoteCodexReloadMode::AppServices, &malformed);
        assert_eq!(parsed.status, RemoteCodexReloadStatus::Failed);

        for contradictory in [
            "CODEXHUB_RELOAD_STATUS=not-running\nCODEXHUB_RELOAD_TARGETED=0\nCODEXHUB_RELOAD_STOPPED=0\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no",
            "CODEXHUB_RELOAD_STATUS=not-running\nCODEXHUB_RELOAD_TARGETED=1\nCODEXHUB_RELOAD_STOPPED=0\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=no-matching-process",
            "CODEXHUB_RELOAD_STATUS=reloaded\nCODEXHUB_RELOAD_TARGETED=0\nCODEXHUB_RELOAD_STOPPED=0\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=processes-stopped",
            "CODEXHUB_RELOAD_STATUS=reconnected\nCODEXHUB_RELOAD_TARGETED=1\nCODEXHUB_RELOAD_STOPPED=1\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=replacement-observed",
            "CODEXHUB_RELOAD_STATUS=reloaded\nCODEXHUB_RELOAD_TARGETED=1\nCODEXHUB_RELOAD_STOPPED=1\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=yes\nCODEXHUB_RELOAD_REASON=processes-stopped",
            "CODEXHUB_RELOAD_STATUS=manual-required\nCODEXHUB_RELOAD_TARGETED=1\nCODEXHUB_RELOAD_STOPPED=2\nCODEXHUB_RELOAD_PRESERVED_CLI=0\nCODEXHUB_RELOAD_REPLACEMENT_OBSERVED=no\nCODEXHUB_RELOAD_REASON=term-failed",
        ] {
            let output = ssh::SshCommandOutput {
                command: "ssh lab sh -s".into(),
                stdout: contradictory.into(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms: 10,
                timed_out: false,
            };
            let parsed = parse_remote_codex_reload_result(
                RemoteCodexReloadMode::AppServices,
                &output,
            );
            assert_eq!(parsed.status, RemoteCodexReloadStatus::Failed);
        }
    }

    #[test]
    fn profile_apply_reload_contract_defaults_and_outcomes_are_stable() {
        let options: ProfileApplyOptions =
            serde_json::from_str("{}").expect("default profile apply options");
        assert_eq!(
            options.remote_codex_reload_mode,
            RemoteCodexReloadMode::AppServices
        );

        let result =
            |config_status: &str, reload_status: RemoteCodexReloadStatus| ProfileApplyHostResult {
                host_id: "host-1".into(),
                host_name: "Host 1".into(),
                host_alias: "lab".into(),
                status: config_status.into(),
                target_path: "~/.codex/config.toml".into(),
                backup_path: None,
                message: "test".into(),
                reload: RemoteCodexReloadResult {
                    mode: RemoteCodexReloadMode::AppServices,
                    status: reload_status,
                    targeted_count: 0,
                    stopped_count: 0,
                    preserved_cli_count: 0,
                    replacement_observed: false,
                    message: "test".into(),
                },
                task: None,
            };

        assert_eq!(
            profile_apply_batch_outcome(&[result("success", RemoteCodexReloadStatus::Reconnected)]),
            ProfileApplyOutcome::Success
        );
        for completed in [
            RemoteCodexReloadStatus::NotRequested,
            RemoteCodexReloadStatus::NotRunning,
            RemoteCodexReloadStatus::Reloaded,
        ] {
            assert_eq!(
                profile_apply_batch_outcome(&[result("no-change", completed)]),
                ProfileApplyOutcome::Success
            );
        }
        assert_eq!(
            profile_apply_batch_outcome(&[result(
                "success",
                RemoteCodexReloadStatus::ManualRequired
            )]),
            ProfileApplyOutcome::ManualReconnect
        );
        assert_eq!(
            profile_apply_batch_outcome(&[result("success", RemoteCodexReloadStatus::Failed)]),
            ProfileApplyOutcome::ManualReconnect
        );
        assert_eq!(
            profile_apply_batch_outcome(&[
                result("success", RemoteCodexReloadStatus::NotRunning),
                result("failed", RemoteCodexReloadStatus::Skipped),
            ]),
            ProfileApplyOutcome::Partial
        );
        assert_eq!(
            profile_apply_batch_outcome(&[result("failed", RemoteCodexReloadStatus::Skipped)]),
            ProfileApplyOutcome::Failed
        );
    }

    #[test]
    fn profile_apply_persists_confirmed_state_before_process_reload() {
        let source = normalized_fixture_source(include_str!("services/profile_operations.rs"));
        let env_check = source
            .find("let api_key_env_present")
            .expect("API env verification");
        let local_persist = source
            .find("let local_persist_error")
            .expect("local confirmed-state persistence");
        let reload = source
            .find("let reload = if remote_apply_ready")
            .expect("remote process reload");
        let cleanup = source
            .find("let cleanup_hard_failed = if remote_apply_ready")
            .expect("post-reload managed release cleanup");

        assert!(env_check < local_persist);
        assert!(local_persist < reload);
        assert!(reload < cleanup);
        assert!(source[local_persist..reload].contains("update_host_profile_apply"));
        assert!(source.contains(
            "profile_apply_task_status(config_succeeded, reload_succeeded, cleanup_hard_failed)"
        ));
        assert!(source.contains("let hard_failed = result.hard_failed()"));
        assert!(matches!(
            profile_apply_task_status(true, true, false),
            TaskStatus::Success
        ));
        assert!(matches!(
            profile_apply_task_status(true, true, true),
            TaskStatus::Failed
        ));
    }

    #[test]
    fn install_runtime_reconcile_and_cleanup_order_is_stable() {
        let source = normalized_fixture_source(include_str!("services/host_operations.rs"));
        let reconcile_guard = source
            .find("let should_reconcile_runtime = successful_install.is_some() || has_runtime_recovery_floor")
            .expect("runtime recovery guard");
        let reconcile = source
            .find("let runtime_reconcile_result = if should_reconcile_runtime")
            .expect("runtime reconcile phase");
        let final_verify = source[reconcile..]
            .find("let after_checks = run_codex_state_checks_parallel")
            .map(|index| reconcile + index)
            .expect("final verification phase");
        let cleanup = source[final_verify..]
            .find("let cleanup_ok = if verification_ok")
            .map(|index| final_verify + index)
            .expect("release cleanup phase");
        assert!(reconcile < final_verify);
        assert!(final_verify < cleanup);
        assert!(reconcile_guard < reconcile);
        assert!(source[reconcile..final_verify].contains("before_version.as_deref()"));
        assert!(source[reconcile..final_verify].contains("strict_current_runtime.as_ref()"));
        assert!(source.contains(
            "let has_runtime_recovery_floor = before_version.is_some() || strict_current_runtime.is_some()"
        ));
        assert!(source.contains(
            "let detected_installed =\n        detected_codex_installed(codex_path.is_some(), after_version.is_some())"
        ));
        assert!(source.contains("&alias,\n        detected_installed,"));
        assert!(
            source.contains("final_verification_failures(\n        successful_install.is_some(),")
        );
        assert!(source.contains("CodexReleaseCleanupStatus::Deferred => TaskStepStatus::Skipped"));
        assert!(source.contains("let cleanup_ok = !result.hard_failed()"));
        assert!(source[cleanup..].contains("action == RemoteCodexAction::Update"));
        assert!(source[cleanup..].contains("CodexReleaseCleanupPolicy::VerifiedOlderThan"));
        assert!(source[cleanup..].contains("CodexReleaseCleanupPolicy::ManagedOnly"));
        let profile_source =
            normalized_fixture_source(include_str!("services/profile_operations.rs"));
        assert!(profile_source.contains("codex_runtime::CodexReleaseCleanupPolicy::ManagedOnly"));
        assert!(!profile_source.contains("CodexReleaseCleanupPolicy::VerifiedOlderThan"));
    }

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
        assert_eq!(
            serde_json::to_string(&RemoteCodexAction::Uninstall).expect("serialize"),
            "\"uninstall\""
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
    fn codex_install_methods_are_separate_safe_user_installers() {
        for script in [
            CODEX_OFFICIAL_INSTALL_SCRIPT,
            CODEX_REMOTE_NATIVE_MIRROR_SCRIPT,
            CODEX_REMOTE_NPM_MIRROR_SCRIPT,
        ] {
            assert!(script.contains("CODEX_INSTALL_DIR=\"$HOME/.local/bin\""));
            assert!(script.contains("CODEX_HOME=\"$HOME/.codex\""));
            assert!(!script.contains("sudo"));
            assert!(!script.contains("chown"));
            assert!(!script.contains("/usr/local/bin"));
        }

        assert!(CODEX_OFFICIAL_INSTALL_SCRIPT.contains("https://chatgpt.com/codex/install.sh"));
        assert!(!CODEX_OFFICIAL_INSTALL_SCRIPT.contains("curl -k"));
        assert!(!CODEX_OFFICIAL_INSTALL_SCRIPT.contains("--no-check-certificate"));
        assert!(CODEX_OFFICIAL_INSTALL_SCRIPT.contains("CODEXHUB_INSTALL_METHOD=official"));
        let official_query = CODEX_OFFICIAL_INSTALL_SCRIPT
            .find("https://api.github.com/repos/openai/codex/releases/latest")
            .expect("query official release before installer mutation");
        let official_floor = CODEX_OFFICIAL_INSTALL_SCRIPT
            .find("codexhub_version_meets_floors \"$candidate_version\"")
            .expect("compare official release against floors");
        let official_pin = CODEX_OFFICIAL_INSTALL_SCRIPT
            .find("export CODEX_RELEASE=\"$candidate_version\"")
            .expect("pin the official installer release");
        let official_run = CODEX_OFFICIAL_INSTALL_SCRIPT
            .find("timeout 75 sh \"$tmp_dir/install.sh\"")
            .expect("run pinned official installer");
        assert!(official_query < official_floor);
        assert!(official_floor < official_pin && official_pin < official_run);

        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT
            .contains("https://registry.npmmirror.com/@openai/codex"));
        assert!(
            CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native")
        );
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT
            .contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native-insecure-tls"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT
            .contains("Insecure TLS fallback is limited to npmmirror URLs"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("phase_label=\"${4:-download}\""));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT
            .contains("npmmirror metadata response was HTML instead of JSON"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("not a readable gzip tarball"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("archive contains unsafe paths"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("curl -k -fsSL"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("--no-check-certificate"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("vendor/$target"));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT.contains("ln -sfn \"$release_dir/bin/codex\""));
        assert!(CODEX_REMOTE_NATIVE_MIRROR_SCRIPT
            .contains("codexhub_version_meets_floors \"$version\""));

        assert!(CODEX_REMOTE_NPM_MIRROR_SCRIPT.contains("command -v npm"));
        assert!(CODEX_REMOTE_NPM_MIRROR_SCRIPT.contains("registry=https://registry.npmmirror.com"));
        let npm_query = CODEX_REMOTE_NPM_MIRROR_SCRIPT
            .find("npm view @openai/codex version")
            .expect("query mirror version before mutation");
        let npm_floor = CODEX_REMOTE_NPM_MIRROR_SCRIPT
            .find("codexhub_version_meets_floors \"$candidate_version\"")
            .expect("compare mirror version against floors");
        let npm_install = CODEX_REMOTE_NPM_MIRROR_SCRIPT
            .find("npm install -g \"@openai/codex@$candidate_version\"")
            .expect("install exact validated mirror version");
        assert!(npm_query < npm_floor && npm_floor < npm_install);
        assert!(!CODEX_REMOTE_NPM_MIRROR_SCRIPT.contains("npm install -g @openai/codex --prefix"));
        assert!(CODEX_REMOTE_NPM_MIRROR_SCRIPT.contains("CODEXHUB_INSTALL_METHOD=npm-mirror"));
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
    fn native_installers_mark_verified_releases_and_never_blindly_replace_same_version() {
        let uploaded = codex_install_uploaded_package_script(
            "/tmp/codexhub-codex-1-0.144.5.tgz",
            "0.144.5",
            "x86_64-unknown-linux-musl",
        );
        for script in [CODEX_REMOTE_NATIVE_MIRROR_SCRIPT, uploaded.as_str()] {
            assert!(script.contains(".codexhub-managed-release"));
            assert!(script.contains("CodexHub managed standalone release v1"));
            assert!(script.contains("write_verified_marker"));
            assert!(script.contains("is_safe_existing_release"));
            assert!(
                script.contains("Existing same-version release directory is not safely adoptable")
            );
            assert!(!script.contains("rm -rf \"$release_dir\""));
            let dual_layout_guard = script
                .find("[ \"$codexhub_existing_layout_count\" -eq 1 ] || return 1")
                .expect("same-version direct layout uniqueness guard");
            let layout_presence_check = script
                .find("if [ -e \"$codexhub_existing_binary\" ] || [ -L \"$codexhub_existing_binary\" ]; then")
                .expect("all direct layout paths count toward ambiguity");
            let strict_binary_check = script
                .find("[ -f \"$codexhub_existing_selected_binary\" ] && [ -x \"$codexhub_existing_selected_binary\" ] && [ ! -L \"$codexhub_existing_selected_binary\" ] || return 1")
                .expect("unique direct layout binary identity guard");
            let canonical_binary_check = script
                .find("[ \"$codexhub_existing_binary_real\" = \"$codexhub_existing_real/$codexhub_existing_selected_relative\" ] || return 1")
                .expect("unique direct layout canonical path guard");
            let current_switch = script
                .find("ln -sfn \"$release_dir\" \"$CODEX_HOME/packages/standalone/current\"")
                .expect("standalone/current switch");
            assert!(layout_presence_check < dual_layout_guard);
            assert!(dual_layout_guard < strict_binary_check);
            assert!(strict_binary_check < canonical_binary_check);
            assert!(canonical_binary_check < current_switch);
            assert!(script
                .contains("[ \"$codexhub_existing_selected_relative\" = bin/codex ] || return 1"));
            for forbidden in [
                "\n  value=$1\n",
                "\n  binary=$1\n",
                "\n  release_dir=$1\n",
                "\n  release_version=$2\n",
                "\n  marker=\"$release_dir/",
                "\n  direct_layout_count=0\n",
            ] {
                assert!(
                    !script.contains(forbidden),
                    "native installer helper leaks POSIX shell state: {forbidden}"
                );
            }
        }
    }

    #[test]
    fn native_installer_helpers_preserve_outer_release_state_during_stage_commit_and_retry() {
        use std::io::Write as _;
        use std::process::Stdio;

        let uploaded = codex_install_uploaded_package_script(
            "/tmp/codexhub-codex-1-0.144.6.tgz",
            "0.144.6",
            "x86_64-unknown-linux-musl",
        );
        for (label, script, helper_end, safe_call) in [
            (
                "remote native",
                CODEX_REMOTE_NATIVE_MIRROR_SCRIPT,
                "extract_npmmirror_metadata() {",
                "is_safe_existing_release \"$release_dir\" \"$version\" \"$release_root\"",
            ),
            (
                "uploaded native",
                uploaded.as_str(),
                "if [ ! -s \"$remote_tarball\" ]; then",
                "is_safe_existing_release \"$release_dir\" \"$version\"",
            ),
        ] {
            let helper_start = script
                .find("binary_version() {")
                .expect("native binary helper");
            let helper_end = script[helper_start..]
                .find(helper_end)
                .map(|index| helper_start + index)
                .expect("native helper boundary");
            let helpers = &script[helper_start..helper_end];
            let harness = r###"set -eu
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT HUP INT TERM
if ! command -v chmod >/dev/null 2>&1; then chmod() { :; }; fi
release_root="$root/releases"
mkdir -p "$release_root"
version=0.144.6
expected_release_dir="$release_root/$version"
release_dir=$expected_release_dir
stage_dir="$release_dir.tmp.123"
marker_name=.codexhub-managed-release
release_version=outer-release-version
marker=outer-marker
marker_tmp=outer-marker-tmp
value=outer-value
binary=outer-binary
direct_layout_count=outer-layout-count
selected_direct_binary=outer-selected-binary
selected_direct_relative=outer-selected-relative
direct_relative=outer-relative
direct_binary=outer-direct-binary
release_root_real=outer-root-real
release_real=outer-release-real
direct_binary_real=outer-direct-real
existing_version=outer-existing-version

__HELPERS__

assert_outer_state() {
  [ "$release_dir" = "$expected_release_dir" ]
  [ "$release_root" = "$root/releases" ]
  [ "$release_version" = outer-release-version ]
  [ "$marker" = outer-marker ]
  [ "$marker_tmp" = outer-marker-tmp ]
  [ "$value" = outer-value ]
  [ "$binary" = outer-binary ]
  [ "$direct_layout_count" = outer-layout-count ]
  [ "$selected_direct_binary" = outer-selected-binary ]
  [ "$selected_direct_relative" = outer-selected-relative ]
  [ "$direct_relative" = outer-relative ]
  [ "$direct_binary" = outer-direct-binary ]
  [ "$release_root_real" = outer-root-real ]
  [ "$release_real" = outer-release-real ]
  [ "$direct_binary_real" = outer-direct-real ]
  [ "$existing_version" = outer-existing-version ]
}

make_stage() {
  destination=$1
  mkdir -p "$destination/bin"
  cat >"$destination/bin/codex" <<'CODEXHUB_TEST_BIN'
#!/bin/sh
printf 'codex-cli 0.144.6\n'
CODEXHUB_TEST_BIN
  chmod 700 "$destination/bin/codex"
}

make_stage "$stage_dir"
write_verified_marker "$stage_dir" "$version"
assert_outer_state
[ ! -e "$release_dir" ] && [ ! -L "$release_dir" ]
mv "$stage_dir" "$release_dir"
stage_dir=""
__SAFE_CALL__
assert_outer_state

retry_stage="$expected_release_dir.tmp.retry"
make_stage "$retry_stage"
write_verified_marker "$retry_stage" "$version"
assert_outer_state
[ -d "$release_dir" ]
__SAFE_CALL__
assert_outer_state
printf 'CODEXHUB_TEST_NATIVE_HELPERS=ok\n'
"###
            .replace("__HELPERS__", helpers)
            .replace("__SAFE_CALL__", safe_call);

            let mut child = match std::process::Command::new("sh")
                .arg("-s")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) => panic!("could not start native helper fixture: {error}"),
            };
            child
                .stdin
                .take()
                .expect("native helper stdin")
                .write_all(harness.as_bytes())
                .expect("write native helper fixture");
            let output = child
                .wait_with_output()
                .expect("wait for native helper fixture");
            assert!(
                output.status.success(),
                "{label} helper fixture failed: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("CODEXHUB_TEST_NATIVE_HELPERS=ok")
            );
        }
    }

    #[test]
    fn every_codexhub_runtime_writer_uses_the_shared_remote_lock() {
        let uploaded = codex_install_uploaded_package_script(
            "/tmp/codexhub-codex-1-0.144.5.tgz",
            "0.144.5",
            "x86_64-unknown-linux-musl",
        );
        for body in [
            CODEX_OFFICIAL_INSTALL_SCRIPT,
            CODEX_REMOTE_NATIVE_MIRROR_SCRIPT,
            CODEX_REMOTE_NPM_MIRROR_SCRIPT,
            CODEX_UNINSTALL_SCRIPT,
            uploaded.as_str(),
        ] {
            let locked =
                crate::services::codex_runtime::with_remote_codex_runtime_writer_lock(body);
            let acquire = locked
                .find("codexhub_runtime_lock_acquire\ncodexhub_runtime_lock_status=$?")
                .expect("writer lock acquisition");
            let body_start = locked.find(body).expect("wrapped runtime writer body");
            assert!(acquire < body_start);
            assert!(locked.contains("CodexHub runtime cleanup lock v1"));
            assert!(locked.contains("codexhub_runtime_lock_root=\"$HOME\""));
            assert!(!locked.contains("codexhub_runtime_lock_path=\"$HOME/.codex-hub/"));
            assert!(locked.contains("codexhub_runtime_mv_no_replace_supported"));
            assert!(locked.contains("trap 'exit 143' TERM"));
        }

        let host_source = normalized_fixture_source(include_str!("services/host_operations.rs"));
        for required in [
            "with_remote_codex_runtime_writer_lock(CODEX_UNINSTALL_SCRIPT)",
            "CODEX_OFFICIAL_INSTALL_SCRIPT\n                    )",
            "CODEX_REMOTE_NATIVE_MIRROR_SCRIPT\n                    )",
            "CODEX_REMOTE_NPM_MIRROR_SCRIPT\n                    )",
            "with_remote_codex_runtime_writer_lock(&install)",
        ] {
            assert!(
                host_source.contains(required),
                "missing runtime writer wrapper: {required}"
            );
        }
        let reconcile = crate::services::codex_runtime::remote_codex_runtime_reconcile_script();
        assert!(
            reconcile.find("codexhub_runtime_lock_acquire").unwrap()
                < reconcile
                    .find("target_file=\"$hub_dir/codex-target\"")
                    .unwrap()
        );
    }

    #[test]
    fn native_installer_scripts_are_posix_shell_syntax() {
        use std::io::Write as _;
        use std::process::Stdio;

        let uploaded = codex_install_uploaded_package_script(
            "/tmp/codexhub-codex-1-0.144.5.tgz",
            "0.144.5",
            "x86_64-unknown-linux-musl",
        );
        for script in [CODEX_REMOTE_NATIVE_MIRROR_SCRIPT, uploaded.as_str()] {
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
                .expect("write native installer script to sh");
            assert!(child.wait().expect("wait for sh -n").success());
        }
    }

    #[test]
    fn codex_path_repair_script_is_managed_idempotent_and_backed_up() {
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("mkdir -p \"$local_bin\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("# >>> CodexHub managed PATH"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("# <<< CodexHub managed PATH"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("changed=no"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("changed=yes"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$shell_config\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$HOME/.profile\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$HOME/.bash_profile\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$HOME/.zprofile\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("cp -p \"$target\" \"$backup_path\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("grep -F \"$path_line\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("CODEXHUB_PATH_CHANGED=%s"));
        assert!(!CODEX_PATH_REPAIR_SCRIPT.contains("sudo"));

        let backup_index = CODEX_PATH_REPAIR_SCRIPT
            .find("cp -p \"$target\" \"$backup_path\"")
            .expect("backup command");
        let append_index = CODEX_PATH_REPAIR_SCRIPT
            .find(">>\"$target\"")
            .expect("append command");
        assert!(backup_index < append_index);
    }

    #[test]
    fn remote_profile_api_key_script_only_writes_managed_env() {
        let script = remote_profile_api_key_script("OPENAI_API_KEY", "sk-test'value");

        assert!(script.contains("env_file=\"$env_dir/env\""));
        assert!(script.contains("printf 'export %s=%s\\n' \"$env_name\" \"$env_value\""));
        assert!(script.contains("env_value='"));
        assert!(script.contains("\"'\""));
        assert!(!script.contains("env_value='sk-test'value'"));
        assert!(script.contains("chmod 600 \"$env_file\""));
        assert!(script.contains("repair_source_file \"$HOME/.profile\""));
        assert!(script.contains("repair_source_file \"$HOME/.bash_profile\""));
        assert!(script.contains("repair_source_file \"$HOME/.zprofile\""));
        assert!(script.contains("CODEXHUB_REMOTE_ENV_CHANGED=%s"));
        assert!(!script.contains("CodexHub managed launcher"));
        assert!(!script.contains("codex-target"));
        assert!(!script.contains("CODEXHUB_CODEX_LAUNCHER"));
    }

    #[test]
    fn profile_runtime_reconcile_runs_only_after_successful_env_write() {
        use std::cell::Cell;

        let output = |success| ssh::SshCommandOutput {
            command: "ssh test profile env".into(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(if success { 0 } else { 1 }),
            duration_ms: 1,
            timed_out: false,
        };
        let called = Cell::new(false);
        let skipped = reconcile_after_successful_remote_env_write(&output(false), || {
            called.set(true);
            unreachable!("failed env write must skip runtime reconciliation")
        });
        assert!(skipped.is_none());
        assert!(!called.get());

        let completed = reconcile_after_successful_remote_env_write(&output(true), || {
            called.set(true);
            Ok(services::codex_runtime::CodexRuntimeReconcileResult {
                status: services::codex_runtime::CodexRuntimeReconcileStatus::NotInstalled,
                target_changed: false,
                launcher_changed: false,
                target_version: None,
                launcher_version: None,
                login_shell_version: None,
                release_marked: false,
                reason: "no-codex-entry".into(),
            })
        });
        assert!(called.get());
        assert!(completed.expect("reconcile result").is_ok());
    }
}
