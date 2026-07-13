use crate::services::updater_operations::*;
use crate::*;

#[cfg(test)]
mod tests {
    use super::*;

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
    fn profile_and_local_skill_tasks_use_expected_public_shapes() {
        let host = test_host("lab");
        let profile_task = profile_apply_task(
            "task-profile-1",
            &host,
            TaskStatus::Success,
            "Applied profile.",
            vec![basic_log("task-profile-1", 0, TaskLogLevel::Info, "done")],
        );
        let skill_task = local_skill_task("Install skill", "Installed local skill.", true);

        assert_eq!(profile_task.action, "Apply profile");
        assert_eq!(profile_task.host_id, "host-lab");
        assert_eq!(
            serde_json::to_string(&profile_task.status).expect("serialize profile status"),
            "\"success\""
        );
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

    #[test]
    fn safe_reconnect_matching_is_single_process_only() {
        let one = "123 codex /home/me/.local/bin/codex app-server --port 1234\n999 bash bash\n";
        assert_eq!(
            safe_reconnect_decision_from_ps(one),
            SafeReconnectDecision::Terminate(123)
        );

        let ambiguous = "123 codex codex app-server\n124 codex codex remote-control\n";
        assert_eq!(
            safe_reconnect_decision_from_ps(ambiguous),
            SafeReconnectDecision::Manual("ambiguous-process-match".into())
        );

        let unsafe_match = "222 node node app-server\n333 codex codex login\n";
        assert_eq!(
            safe_reconnect_decision_from_ps(unsafe_match),
            SafeReconnectDecision::Manual("no-safe-process-match".into())
        );
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

        assert!(CODEX_REMOTE_NPM_MIRROR_SCRIPT.contains("command -v npm"));
        assert!(CODEX_REMOTE_NPM_MIRROR_SCRIPT.contains("registry=https://registry.npmmirror.com"));
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
    fn remote_profile_api_key_script_writes_managed_env_and_launcher() {
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
        assert!(script.contains("CodexHub managed launcher"));
        assert!(script.contains("target_file=\"$HOME/.codex-hub/codex-target\""));
        assert!(script.contains("exec \"$target\" \"$@\""));
        assert!(script.contains("CODEXHUB_REMOTE_ENV_CHANGED=%s"));
        assert!(script.contains("CODEXHUB_CODEX_LAUNCHER_CHANGED=%s"));
    }
}
