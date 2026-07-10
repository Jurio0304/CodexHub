use crate::*;

pub(crate) fn find_profile(
    app: &AppHandle,
    state: &AppState,
    profile_id: &str,
) -> Result<Profile, String> {
    load_profiles(app, state)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile {profile_id} was not found."))
}

pub(crate) fn profile_from_draft(draft: ProfileDraft) -> Result<Profile, String> {
    let now = timestamp_label();
    let name = normalize_required_text("Profile name", &draft.name)?;
    let model = normalize_required_text("Model", &draft.model)?;
    Ok(Profile {
        id: format!("{}-{}", slugify(&name), timestamp_millis()),
        name,
        description: draft.description.unwrap_or_default(),
        model,
        provider: normalize_optional_text(draft.provider).unwrap_or_else(|| "openai".into()),
        base_url: normalize_optional_text(draft.base_url),
        api_key_env_var: normalize_optional_text(draft.api_key_env_var),
        model_reasoning_effort: normalize_optional_text(draft.model_reasoning_effort),
        plan_mode_reasoning_effort: normalize_optional_text(draft.plan_mode_reasoning_effort),
        fast_mode: draft.fast_mode.unwrap_or(false),
        service_tier: normalize_optional_text(draft.service_tier),
        approval_policy: normalize_optional_text(draft.approval_policy)
            .unwrap_or_else(|| "on-request".into()),
        sandbox_mode: normalize_optional_text(draft.sandbox_mode)
            .unwrap_or_else(|| "workspace-write".into()),
        extra_toml: draft.extra_toml.unwrap_or_default(),
        created_at: now.clone(),
        updated_at: now,
        source: normalize_optional_text(draft.source).unwrap_or_else(|| "manual".into()),
        credential_stored: false,
        host_ids: draft.host_ids.unwrap_or_default(),
    })
}

pub(crate) fn apply_profile_patch(
    profile: &mut Profile,
    patch: ProfilePatch,
) -> Result<(), String> {
    if let Some(name) = patch.name {
        profile.name = normalize_required_text("Profile name", &name)?;
    }
    if let Some(description) = patch.description {
        profile.description = description;
    }
    if let Some(model) = patch.model {
        profile.model = normalize_required_text("Model", &model)?;
    }
    if let Some(provider) = patch.provider {
        profile.provider = normalize_required_text("Provider", &provider)?;
    }
    if patch.base_url.is_some() {
        profile.base_url = normalize_optional_text(patch.base_url);
    }
    if patch.api_key_env_var.is_some() {
        profile.api_key_env_var = normalize_optional_text(patch.api_key_env_var);
    }
    if patch.model_reasoning_effort.is_some() {
        profile.model_reasoning_effort = normalize_optional_text(patch.model_reasoning_effort);
    }
    if patch.plan_mode_reasoning_effort.is_some() {
        profile.plan_mode_reasoning_effort =
            normalize_optional_text(patch.plan_mode_reasoning_effort);
    }
    if let Some(fast_mode) = patch.fast_mode {
        profile.fast_mode = fast_mode;
    }
    if patch.service_tier.is_some() {
        profile.service_tier = normalize_optional_text(patch.service_tier);
    }
    if let Some(approval_policy) = patch.approval_policy {
        profile.approval_policy = normalize_required_text("Approval policy", &approval_policy)?;
    }
    if let Some(sandbox_mode) = patch.sandbox_mode {
        profile.sandbox_mode = normalize_required_text("Sandbox mode", &sandbox_mode)?;
    }
    if let Some(extra_toml) = patch.extra_toml {
        profile.extra_toml = extra_toml;
    }
    if let Some(source) = patch.source {
        profile.source = normalize_required_text("Source", &source)?;
    }
    if let Some(credential_stored) = patch.credential_stored {
        profile.credential_stored = credential_stored && profile_api_key_exists(&profile.id)?;
    }
    if let Some(host_ids) = patch.host_ids {
        profile.host_ids = host_ids;
    }
    Ok(())
}

pub(crate) fn validate_profile(profile: &Profile) -> Result<(), String> {
    normalize_required_text("Profile id", &profile.id)?;
    normalize_required_text("Profile name", &profile.name)?;
    normalize_required_text("Model", &profile.model)?;
    normalize_required_text("Provider", &profile.provider)?;
    normalize_required_text("Approval policy", &profile.approval_policy)?;
    normalize_required_text("Sandbox mode", &profile.sandbox_mode)?;
    validate_extra_toml(profile)?;
    let rendered = serde_json::to_string(profile).map_err(|error| error.to_string())?;
    if contains_key_material(&rendered) {
        return Err("Profile contains data that looks like API key material.".into());
    }
    Ok(())
}

pub(crate) fn ensure_unique_profile_id(profile: &mut Profile, profiles: &[Profile]) {
    if !profiles.iter().any(|item| item.id == profile.id) {
        return;
    }
    let base = profile.id.clone();
    let mut index = 2;
    while profiles
        .iter()
        .any(|item| item.id == format!("{base}-{index}"))
    {
        index += 1;
    }
    profile.id = format!("{base}-{index}");
}

pub(crate) fn import_profiles_inner(
    app: &AppHandle,
    state: &AppState,
    incoming: Vec<Profile>,
    replace: bool,
) -> Result<ProfileImportResult, String> {
    let (profiles, result) = prepare_profiles_import(app, state, incoming, replace)?;
    save_profiles(app, state, &profiles)?;
    Ok(result)
}

/// Builds the complete profile snapshot without mutating JSON. Callers that
/// also change OS credentials can commit both sides through a compensation
/// service after every input has been validated.
pub(crate) fn prepare_profiles_import(
    app: &AppHandle,
    state: &AppState,
    incoming: Vec<Profile>,
    replace: bool,
) -> Result<(Vec<Profile>, ProfileImportResult), String> {
    let mut profiles = if replace {
        Vec::new()
    } else {
        load_profiles(app, state)?
    };
    let mut imported = Vec::new();
    let mut skipped = Vec::new();

    for mut profile in incoming {
        profile.credential_stored = profile_api_key_exists(&profile.id)?;
        profile.updated_at = timestamp_label();
        if profile.created_at.trim().is_empty() {
            profile.created_at = profile.updated_at.clone();
        }
        match validate_profile(&profile) {
            Ok(()) => {
                profiles.retain(|item| item.id != profile.id);
                if profile.source == "cc-switch" {
                    let incoming_key = cc_switch_profile_import_key(&profile);
                    profiles.retain(|item| {
                        item.source != "cc-switch"
                            || cc_switch_profile_import_key(item) != incoming_key
                    });
                }
                profiles.push(profile.clone());
                imported.push(profile);
            }
            Err(error) => skipped.push(format!("{}: {error}", profile.id)),
        }
    }

    Ok((profiles, ProfileImportResult { imported, skipped }))
}

pub(crate) fn refresh_credential_flags(profiles: &mut [Profile]) -> Result<(), String> {
    for profile in profiles {
        profile.credential_stored = profile_api_key_exists(&profile.id)?;
    }
    Ok(())
}

pub(crate) fn clear_profile_from_host_list(hosts: &mut [Host], profile_id: &str) {
    for host in hosts.iter_mut() {
        if host.profile_id.as_deref() == Some(profile_id) {
            host.profile_id = None;
        }
    }
}

pub(crate) fn render_profile_toml(profile: &Profile) -> Result<String, String> {
    validate_profile(profile)?;
    let mut root = TomlMap::new();
    insert_toml_string(&mut root, "model", &profile.model);
    insert_toml_string(&mut root, "model_provider", &profile.provider);
    insert_toml_string(&mut root, "approval_policy", &profile.approval_policy);
    insert_toml_string(&mut root, "sandbox_mode", &profile.sandbox_mode);
    insert_toml_optional_string(
        &mut root,
        "model_reasoning_effort",
        profile.model_reasoning_effort.as_deref(),
    );
    insert_toml_optional_string(
        &mut root,
        "plan_mode_reasoning_effort",
        profile.plan_mode_reasoning_effort.as_deref(),
    );
    insert_toml_optional_string(&mut root, "service_tier", profile.service_tier.as_deref());

    if profile.provider.eq_ignore_ascii_case("openai") {
        insert_toml_optional_string(&mut root, "openai_base_url", profile.base_url.as_deref());
    } else {
        let provider_key = sanitize_toml_key(&profile.provider)?;
        let mut provider = TomlMap::new();
        insert_toml_string(&mut provider, "name", &profile.provider);
        insert_toml_optional_string(&mut provider, "base_url", profile.base_url.as_deref());
        insert_toml_optional_string(&mut provider, "env_key", profile.api_key_env_var.as_deref());
        let mut provider_tables = TomlMap::new();
        provider_tables.insert(provider_key, TomlValue::Table(provider));
        root.insert("model_providers".into(), TomlValue::Table(provider_tables));
    }

    let mut features = TomlMap::new();
    features.insert("fast_mode".into(), TomlValue::Boolean(profile.fast_mode));
    root.insert("features".into(), TomlValue::Table(features));

    let extra = parse_extra_toml(profile)?;
    merge_toml_table(&mut root, extra)?;
    toml::to_string_pretty(&TomlValue::Table(root)).map_err(|error| error.to_string())
}

pub(crate) fn parse_extra_toml(profile: &Profile) -> Result<TomlMap<String, TomlValue>, String> {
    let trimmed = profile.extra_toml.trim();
    if trimmed.is_empty() {
        return Ok(TomlMap::new());
    }
    let value = trimmed
        .parse::<TomlValue>()
        .map_err(|error| format!("extraToml is not valid TOML: {error}"))?;
    let TomlValue::Table(table) = value else {
        return Err("extraToml must be a TOML table.".into());
    };
    reject_extra_toml_conflicts(profile, &table)?;
    reject_extra_toml_secret_keys(&table, "")?;
    Ok(table)
}

pub(crate) fn validate_extra_toml(profile: &Profile) -> Result<(), String> {
    parse_extra_toml(profile).map(|_| ())
}

pub(crate) fn reject_extra_toml_conflicts(
    profile: &Profile,
    table: &TomlMap<String, TomlValue>,
) -> Result<(), String> {
    let top_level_conflicts = [
        "model",
        "model_provider",
        "approval_policy",
        "sandbox_mode",
        "model_reasoning_effort",
        "plan_mode_reasoning_effort",
        "service_tier",
        "openai_base_url",
    ];
    for key in top_level_conflicts {
        if table.contains_key(key) {
            return Err(format!("extraToml cannot override structured key `{key}`."));
        }
    }
    if let Some(TomlValue::Table(features)) = table.get("features") {
        if features.contains_key("fast_mode") {
            return Err("extraToml cannot override structured key `features.fast_mode`.".into());
        }
    }
    if let Some(TomlValue::Table(model_providers)) = table.get("model_providers") {
        let provider_key = sanitize_toml_key(&profile.provider)?;
        if model_providers.contains_key(&provider_key) {
            return Err(format!(
                "extraToml cannot override structured key `model_providers.{provider_key}`."
            ));
        }
        if model_providers.contains_key("openai") {
            return Err("extraToml cannot define `model_providers.openai`; OpenAI uses the built-in provider.".into());
        }
    }
    Ok(())
}

pub(crate) fn merge_toml_table(
    target: &mut TomlMap<String, TomlValue>,
    source: TomlMap<String, TomlValue>,
) -> Result<(), String> {
    for (key, value) in source {
        match (target.get_mut(&key), value) {
            (Some(TomlValue::Table(target_table)), TomlValue::Table(source_table)) => {
                merge_toml_table(target_table, source_table)?;
            }
            (Some(_), _) => {
                return Err(format!("extraToml conflicts with structured key `{key}`."));
            }
            (None, value) => {
                target.insert(key, value);
            }
        }
    }
    Ok(())
}

pub(crate) fn reject_extra_toml_secret_keys(
    table: &TomlMap<String, TomlValue>,
    prefix: &str,
) -> Result<(), String> {
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        let key_lower = key.to_ascii_lowercase();
        if matches!(
            key_lower.as_str(),
            "api_key" | "apikey" | "token" | "password" | "secret"
        ) {
            return Err(format!(
                "extraToml cannot include secret-like key `{path}`; store local credentials with set_profile_api_key or reference remote environment variables with env_key."
            ));
        }
        if let TomlValue::Table(child) = value {
            reject_extra_toml_secret_keys(child, &path)?;
        }
    }
    Ok(())
}

pub(crate) fn insert_toml_string(table: &mut TomlMap<String, TomlValue>, key: &str, value: &str) {
    table.insert(key.into(), TomlValue::String(value.to_string()));
}

pub(crate) fn insert_toml_optional_string(
    table: &mut TomlMap<String, TomlValue>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        table.insert(key.into(), TomlValue::String(value.to_string()));
    }
}

pub(crate) fn profile_preview_warnings(profile: &Profile) -> Vec<String> {
    let mut warnings = Vec::new();
    if profile.credential_stored {
        warnings.push("Local credential is stored but will not be rendered or uploaded.".into());
    }
    if !profile.provider.eq_ignore_ascii_case("openai") && profile.api_key_env_var.is_none() {
        warnings.push(
            "Custom provider has no env_key; remote authentication must be handled separately."
                .into(),
        );
    }
    warnings
}

pub(crate) fn detect_cc_switch_profiles_inner(
    state: &AppState,
) -> Result<Vec<DetectedCcSwitchProfile>, String> {
    let mut detected = Vec::new();
    let mut seen = BTreeSet::new();
    for path in cc_switch_candidate_paths(state) {
        if !path.exists() {
            continue;
        }
        let profiles = match path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref()
        {
            Some("db") | Some("sqlite") | Some("sqlite3") => parse_cc_switch_db_profiles(&path)?,
            _ => {
                let content = match fs::read_to_string(&path) {
                    Ok(content) => content,
                    Err(_) => continue,
                };
                parse_cc_switch_profiles(&content, &path)?
            }
        };
        push_detected_cc_switch_profiles(&mut detected, &mut seen, &path, profiles);
    }
    Ok(detected)
}

pub(crate) fn cc_switch_candidate_paths(state: &AppState) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(
        state
            .paths
            .config_dir()
            .join("cc-switch")
            .join("profiles.json"),
    );
    paths.push(
        state
            .paths
            .config_dir()
            .join("cc-switch")
            .join("config.json"),
    );
    if let Some(home) = home_dir() {
        paths.push(home.join(".cc-switch").join("profiles.json"));
        paths.push(home.join(".cc-switch").join("config.json"));
        paths.push(home.join(".cc-switch").join("settings.json"));
        paths.push(home.join(".cc-switch").join("cc-switch.db"));
        paths.push(home.join(".cc-switch").join("cc-switch.sqlite"));
        paths.push(home.join(".config").join("cc-switch").join("profiles.json"));
        paths.push(home.join(".config").join("cc-switch").join("config.json"));
        paths.push(home.join(".config").join("cc-switch").join("settings.json"));
        paths.push(home.join(".config").join("cc-switch").join("cc-switch.db"));
    }
    if let Some(appdata) = env::var_os("APPDATA").map(PathBuf::from) {
        paths.push(appdata.join("com.ccswitch.desktop").join("profiles.json"));
        paths.push(appdata.join("com.ccswitch.desktop").join("config.json"));
        paths.push(appdata.join("com.ccswitch.desktop").join("settings.json"));
        paths.push(appdata.join("cc-switch").join("profiles.json"));
        paths.push(appdata.join("cc-switch").join("config.json"));
    }
    if let Some(localappdata) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        paths.push(
            localappdata
                .join("com.ccswitch.desktop")
                .join("cc-switch.db"),
        );
        paths.push(localappdata.join("cc-switch").join("cc-switch.db"));
    }
    paths
}

pub(crate) fn parse_cc_switch_profiles(
    content: &str,
    path: &Path,
) -> Result<Vec<CcSwitchProfileRecord>, String> {
    let json = match serde_json::from_str::<serde_json::Value>(content) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    let mut entries = Vec::new();
    collect_cc_switch_profile_entries(&json, &mut entries);
    let mut profiles = Vec::new();
    for (index, item) in entries.iter().enumerate() {
        let name = item
            .get("name")
            .or_else(|| item.get("title"))
            .or_else(|| item.get("label"))
            .and_then(|value| value.as_str())
            .unwrap_or("Imported CC Switch Profile");
        let settings = item
            .get("settings")
            .or_else(|| item.get("settingsConfig"))
            .or_else(|| item.get("settings_config"));
        let config = item
            .get("config")
            .or_else(|| settings.and_then(|value| value.get("config")))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let mut from_config = cc_switch_profile_from_config(
            &format!("{}-{}", slugify(name), index + 1),
            name,
            config,
            None,
            path,
        );
        let config_api_key = cc_switch_api_key_from_config(config);
        let model = item
            .get("model")
            .or_else(|| settings.and_then(|value| value.get("model")))
            .and_then(|value| value.as_str())
            .or_else(|| {
                from_config
                    .as_ref()
                    .map(|record| record.profile.model.as_str())
            })
            .unwrap_or("gpt-5-codex");
        let provider = item
            .get("provider")
            .or_else(|| item.get("modelProvider"))
            .or_else(|| item.get("model_provider"))
            .or_else(|| settings.and_then(|value| value.get("provider")))
            .and_then(|value| value.as_str())
            .or_else(|| {
                from_config
                    .as_ref()
                    .map(|record| record.profile.provider.as_str())
            })
            .unwrap_or("openai");
        let now = timestamp_label();
        let profile = Profile {
            id: format!("cc-switch-{}-{}", slugify(name), index + 1),
            name: name.to_string(),
            description: format!("Imported from {}", path.display()),
            model: model.to_string(),
            provider: provider.to_string(),
            base_url: item
                .get("base_url")
                .or_else(|| item.get("baseUrl"))
                .or_else(|| item.get("url"))
                .or_else(|| item.get("websiteUrl"))
                .or_else(|| item.get("website_url"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.base_url.take())
                }),
            api_key_env_var: item
                .get("api_key_env_var")
                .or_else(|| item.get("apiKeyEnvVar"))
                .or_else(|| item.get("env_key"))
                .or_else(|| settings.and_then(|value| value.get("env_key")))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.api_key_env_var.take())
                })
                .or_else(|| Some("OPENAI_API_KEY".into())),
            model_reasoning_effort: item
                .get("model_reasoning_effort")
                .or_else(|| item.get("modelReasoningEffort"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.model_reasoning_effort.take())
                }),
            plan_mode_reasoning_effort: item
                .get("plan_mode_reasoning_effort")
                .or_else(|| item.get("planModeReasoningEffort"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.plan_mode_reasoning_effort.take())
                }),
            fast_mode: item
                .get("fast_mode")
                .or_else(|| item.get("fastMode"))
                .and_then(|value| value.as_bool())
                .unwrap_or_else(|| {
                    from_config
                        .as_ref()
                        .map(|record| record.profile.fast_mode)
                        .unwrap_or(false)
                }),
            service_tier: item
                .get("service_tier")
                .or_else(|| item.get("serviceTier"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.service_tier.take())
                }),
            approval_policy: item
                .get("approval_policy")
                .or_else(|| item.get("approvalPolicy"))
                .and_then(|value| value.as_str())
                .unwrap_or("on-request")
                .to_string(),
            sandbox_mode: item
                .get("sandbox_mode")
                .or_else(|| item.get("sandboxMode"))
                .and_then(|value| value.as_str())
                .unwrap_or("workspace-write")
                .to_string(),
            extra_toml: String::new(),
            created_at: now.clone(),
            updated_at: now,
            source: "cc-switch".into(),
            credential_stored: false,
            host_ids: Vec::new(),
        };
        if validate_profile(&profile).is_ok() {
            profiles.push(CcSwitchProfileRecord {
                api_key: cc_switch_api_key_from_value(item)
                    .or_else(|| settings.and_then(cc_switch_api_key_from_value))
                    .or(config_api_key)
                    .or_else(|| from_config.and_then(|record| record.api_key)),
                profile,
            });
        }
    }
    Ok(profiles)
}

pub(crate) fn collect_cc_switch_profile_entries<'a>(
    value: &'a serde_json::Value,
    entries: &mut Vec<&'a serde_json::Value>,
) {
    if let Some(array) = value.as_array() {
        for item in array {
            collect_cc_switch_profile_entries(item, entries);
        }
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    let app_type = object
        .get("app_type")
        .or_else(|| object.get("appType"))
        .or_else(|| object.get("app"))
        .and_then(|value| value.as_str());
    let has_profile_shape = object.contains_key("model")
        || object.contains_key("provider")
        || object.contains_key("modelProvider")
        || object.contains_key("model_provider")
        || object.contains_key("base_url")
        || object.contains_key("baseUrl")
        || object.contains_key("settings_config")
        || object.contains_key("settingsConfig")
        || object.contains_key("config");
    if has_profile_shape && app_type.map(|value| value == "codex").unwrap_or(true) {
        entries.push(value);
    }
    for key in ["profiles", "providers", "items", "data"] {
        if let Some(child) = object.get(key) {
            collect_cc_switch_profile_entries(child, entries);
        }
    }
}

pub(crate) fn parse_cc_switch_db_profiles(
    path: &Path,
) -> Result<Vec<CcSwitchProfileRecord>, String> {
    let sqlite_profiles = parse_cc_switch_sqlite_profiles(path)?;
    if !sqlite_profiles.is_empty() {
        return Ok(sqlite_profiles);
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&bytes);
    Ok(parse_cc_switch_raw_db_profiles(&text, path))
}

pub(crate) fn parse_cc_switch_sqlite_profiles(
    path: &Path,
) -> Result<Vec<CcSwitchProfileRecord>, String> {
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = match rusqlite::Connection::open_with_flags(path, flags) {
        Ok(connection) => connection,
        Err(_) => return Ok(Vec::new()),
    };
    if !sqlite_table_exists(&connection, "providers")? {
        return Ok(Vec::new());
    }

    let current_provider = read_cc_switch_current_provider(path);
    let mut statement = match connection.prepare(
        "SELECT id, name, settings_config, website_url, is_current \
         FROM providers WHERE app_type = 'codex'",
    ) {
        Ok(statement) => statement,
        Err(_) => return Ok(Vec::new()),
    };

    let rows = statement
        .query_map([], |row| {
            Ok(CcSwitchSqliteProvider {
                id: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                settings_config: row.get::<_, String>(2)?,
                website_url: row.get::<_, Option<String>>(3).ok().flatten(),
                is_current: row.get::<_, bool>(4).unwrap_or(false),
            })
        })
        .map_err(|error| format!("Failed to query cc-switch providers: {error}"))?;

    let mut profiles = Vec::new();
    for row in rows {
        let row = match row {
            Ok(row) => row,
            Err(_) => continue,
        };
        let settings = match serde_json::from_str::<serde_json::Value>(&row.settings_config) {
            Ok(settings) => settings,
            Err(_) => continue,
        };
        let config = settings
            .get("config")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let endpoint = cc_switch_provider_endpoint(&connection, &row.id)
            .unwrap_or(None)
            .or(row.website_url.clone());
        if let Some(mut record) =
            cc_switch_profile_from_config(&row.id, &row.name, config, endpoint.as_deref(), path)
        {
            record.api_key = cc_switch_api_key_from_value(&settings)
                .or_else(|| cc_switch_api_key_from_config(config))
                .or(record.api_key);
            let is_current = row.is_current || current_provider.as_deref() == Some(row.id.as_str());
            profiles.push((is_current, record));
        }
    }

    profiles.sort_by_key(|(is_current, record)| {
        (!*is_current, record.profile.name.to_ascii_lowercase())
    });
    Ok(dedupe_cc_switch_profiles(
        profiles.into_iter().map(|(_, record)| record).collect(),
    ))
}

pub(crate) struct CcSwitchSqliteProvider {
    id: String,
    name: String,
    settings_config: String,
    website_url: Option<String>,
    is_current: bool,
}

pub(crate) fn sqlite_table_exists(
    connection: &rusqlite::Connection,
    table_name: &str,
) -> Result<bool, String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table_name],
            |row| row.get(0),
        )
        .map_err(|error| format!("Failed to inspect cc-switch database schema: {error}"))?;
    Ok(count > 0)
}

pub(crate) fn cc_switch_provider_endpoint(
    connection: &rusqlite::Connection,
    provider_id: &str,
) -> Result<Option<String>, String> {
    if !sqlite_table_exists(connection, "provider_endpoints")? {
        return Ok(None);
    }
    match connection.query_row(
        "SELECT url FROM provider_endpoints \
         WHERE provider_id = ?1 AND app_type = 'codex' \
         ORDER BY id ASC LIMIT 1",
        [provider_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(url) => Ok(Some(url)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(_) => Ok(None),
    }
}

pub(crate) fn read_cc_switch_current_provider(db_path: &Path) -> Option<String> {
    let settings_path = db_path.parent()?.join("settings.json");
    let content = fs::read_to_string(settings_path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    value
        .get("currentProviderCodex")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

pub(crate) fn parse_cc_switch_raw_db_profiles(
    content: &str,
    path: &Path,
) -> Vec<CcSwitchProfileRecord> {
    let mut profiles = Vec::new();
    let mut seen_json_starts = BTreeSet::new();
    for marker in ["{\"auth\"", "{\"env\"", "{\"config\""] {
        let mut offset = 0;
        while let Some(relative) = content[offset..].find(marker) {
            let json_start = offset + relative;
            offset = json_start + marker.len();
            if !seen_json_starts.insert(json_start) {
                continue;
            }
            let Some((json, json_end)) = extract_json_object(content, json_start) else {
                continue;
            };
            let Ok(settings) = serde_json::from_str::<serde_json::Value>(json) else {
                continue;
            };
            let Some((record_id, name)) = cc_switch_raw_record_identity(content, json_start) else {
                continue;
            };
            let config = settings
                .get("config")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let fallback_url = extract_url_after(content, json_end);
            if let Some(mut record) = cc_switch_profile_from_config(
                &record_id,
                &name,
                config,
                fallback_url.as_deref(),
                path,
            ) {
                record.api_key = cc_switch_api_key_from_value(&settings)
                    .or_else(|| cc_switch_api_key_from_config(config))
                    .or(record.api_key);
                profiles.push(record);
            }
        }
    }
    dedupe_cc_switch_profiles(profiles)
}

pub(crate) fn extract_json_object(content: &str, start: usize) -> Option<(&str, usize)> {
    if !content[start..].starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in content[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = start + offset + ch.len_utf8();
                    return Some((&content[start..end], end));
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn cc_switch_raw_record_identity(
    content: &str,
    json_start: usize,
) -> Option<(String, String)> {
    let prefix_start = json_start.saturating_sub(260);
    let prefix = &content[prefix_start..json_start];
    let marker = prefix.rfind("codex")?;
    let name = clean_cc_switch_raw_field(&prefix[marker + "codex".len()..]);
    if name.is_empty() || name.contains("session") {
        return None;
    }
    let before_marker = &prefix[..marker];
    let id = last_uuid(before_marker).or_else(|| last_cc_switch_ascii_token(before_marker))?;
    if id.contains("session") {
        return None;
    }
    Some((id, name))
}

pub(crate) fn clean_cc_switch_raw_field(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| {
            if ch.is_control() || ch == '\u{fffd}' {
                ' '
            } else {
                ch
            }
        })
        .collect();
    cleaned
        .trim_matches(|ch: char| {
            !(ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.' | '(' | ')'))
        })
        .trim()
        .to_string()
}

pub(crate) fn last_uuid(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.len() < 36 {
        return None;
    }
    for start in (0..=bytes.len() - 36).rev() {
        let candidate = &value[start..start + 36];
        if is_uuid_like(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

pub(crate) fn is_uuid_like(value: &str) -> bool {
    value.len() == 36
        && value.char_indices().all(|(index, ch)| {
            matches!(index, 8 | 13 | 18 | 23) && ch == '-'
                || !matches!(index, 8 | 13 | 18 | 23) && ch.is_ascii_hexdigit()
        })
}

pub(crate) fn last_cc_switch_ascii_token(value: &str) -> Option<String> {
    let trimmed = value.trim_end_matches(|ch: char| {
        !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    });
    let start = trimmed
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')))
        .map(|index| index + 1)
        .unwrap_or(0);
    let token = trimmed[start..].trim();
    if matches!(token, "default" | "codex-official") || token.ends_with("-official") {
        Some(token.to_string())
    } else {
        None
    }
}

pub(crate) fn extract_url_after(content: &str, start: usize) -> Option<String> {
    let end = (start + 420).min(content.len());
    let window = &content[start..end];
    let http = window.find("http://");
    let https = window.find("https://");
    let offset = match (http, https) {
        (Some(http), Some(https)) => http.min(https),
        (Some(http), None) => http,
        (None, Some(https)) => https,
        (None, None) => return None,
    };
    let url = window[offset..]
        .chars()
        .take_while(|ch| {
            !ch.is_whitespace()
                && !ch.is_control()
                && !matches!(ch, '"' | '\'' | '<' | '>' | '{' | '}' | '[' | ']')
        })
        .collect::<String>()
        .trim_end_matches(|ch| matches!(ch, ',' | ';' | ')' | '('))
        .to_string();
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url)
    } else {
        None
    }
}

pub(crate) fn cc_switch_profile_from_config(
    record_id: &str,
    name: &str,
    config: &str,
    fallback_url: Option<&str>,
    path: &Path,
) -> Option<CcSwitchProfileRecord> {
    let parsed = config.parse::<TomlValue>().ok();
    let root = parsed.as_ref().and_then(TomlValue::as_table);
    let model = root
        .and_then(|table| toml_string(table, "model"))
        .or_else(|| toml_line_string(config, "model"))
        .unwrap_or_else(|| "gpt-5-codex".into());
    let provider = root
        .and_then(|table| toml_string(table, "model_provider"))
        .or_else(|| toml_line_string(config, "model_provider"))
        .unwrap_or_else(|| "openai".into());
    let provider_table = root
        .and_then(|table| table.get("model_providers"))
        .and_then(TomlValue::as_table)
        .and_then(|providers| providers.get(&provider))
        .and_then(TomlValue::as_table);
    let base_url = provider_table
        .and_then(|table| toml_string(table, "base_url"))
        .or_else(|| root.and_then(|table| toml_string(table, "openai_base_url")))
        .or_else(|| toml_line_string(config, "base_url"))
        .or_else(|| fallback_url.map(str::to_string));
    let api_key_env_var = provider_table
        .and_then(|table| toml_string(table, "env_key"))
        .or_else(|| toml_line_string(config, "env_key"))
        .or_else(|| Some("OPENAI_API_KEY".into()));
    let api_key = cc_switch_api_key_from_config(config);
    let model_reasoning_effort = root
        .and_then(|table| toml_string(table, "model_reasoning_effort"))
        .or_else(|| toml_line_string(config, "model_reasoning_effort"));
    let plan_mode_reasoning_effort = root
        .and_then(|table| toml_string(table, "plan_mode_reasoning_effort"))
        .or_else(|| toml_line_string(config, "plan_mode_reasoning_effort"));
    let service_tier = root
        .and_then(|table| toml_string(table, "service_tier"))
        .or_else(|| toml_line_string(config, "service_tier"));
    let fast_mode = root
        .and_then(|table| table.get("features"))
        .and_then(TomlValue::as_table)
        .and_then(|table| table.get("fast_mode"))
        .and_then(TomlValue::as_bool)
        .unwrap_or(false);

    if config.trim().is_empty() && base_url.is_none() && record_id != "codex-official" {
        return None;
    }

    let now = timestamp_label();
    let profile = Profile {
        id: format!("cc-switch-{}", slugify(record_id)),
        name: name.to_string(),
        description: format!("Imported from {}", path.display()),
        model,
        provider,
        base_url,
        api_key_env_var,
        model_reasoning_effort,
        plan_mode_reasoning_effort,
        fast_mode,
        service_tier,
        approval_policy: "on-request".into(),
        sandbox_mode: "workspace-write".into(),
        extra_toml: String::new(),
        created_at: now.clone(),
        updated_at: now,
        source: "cc-switch".into(),
        credential_stored: false,
        host_ids: Vec::new(),
    };
    validate_profile(&profile)
        .ok()
        .map(|_| CcSwitchProfileRecord { profile, api_key })
}

pub(crate) fn cc_switch_api_key_from_value(value: &serde_json::Value) -> Option<String> {
    let direct_candidates = [
        value
            .get("auth")
            .and_then(|auth| auth.get("api_key"))
            .and_then(|item| item.as_str()),
        value
            .get("auth")
            .and_then(|auth| auth.get("apiKey"))
            .and_then(|item| item.as_str()),
        value.get("api_key").and_then(|item| item.as_str()),
        value.get("apiKey").and_then(|item| item.as_str()),
    ];
    direct_candidates
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|item| !item.is_empty())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("auth")
                .and_then(serde_json::Value::as_object)
                .and_then(|auth| {
                    auth.iter()
                        .filter(|(key, _)| cc_switch_auth_key_may_hold_api_key(key))
                        .filter_map(|(_, value)| value.as_str())
                        .map(str::trim)
                        .find(|item| !item.is_empty())
                        .map(str::to_string)
                })
        })
}

pub(crate) fn cc_switch_auth_key_may_hold_api_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.contains("apikey") || normalized.ends_with("token")
}

pub(crate) fn cc_switch_api_key_from_config(config: &str) -> Option<String> {
    let parsed = config.parse::<TomlValue>().ok();
    let root = parsed.as_ref().and_then(TomlValue::as_table);
    let provider = root
        .and_then(|table| toml_string(table, "model_provider"))
        .or_else(|| toml_line_string(config, "model_provider"))
        .unwrap_or_else(|| "openai".into());
    let provider_table = root
        .and_then(|table| table.get("model_providers"))
        .and_then(TomlValue::as_table)
        .and_then(|providers| providers.get(&provider))
        .and_then(TomlValue::as_table);
    provider_table
        .and_then(|table| toml_string(table, "api_key"))
        .or_else(|| root.and_then(|table| toml_string(table, "api_key")))
        .or_else(|| toml_line_string(config, "api_key"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn toml_string(table: &TomlMap<String, TomlValue>, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(TomlValue::as_str)
        .map(str::to_string)
}

pub(crate) fn toml_line_string(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.starts_with(key) {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if left.trim() != key {
            continue;
        }
        let value = right.trim();
        if let Some(stripped) = value
            .strip_prefix('"')
            .and_then(|item| item.strip_suffix('"'))
        {
            return Some(stripped.replace("\\\"", "\""));
        }
    }
    None
}

pub(crate) fn push_detected_cc_switch_profiles(
    detected: &mut Vec<DetectedCcSwitchProfile>,
    seen: &mut BTreeSet<String>,
    path: &Path,
    profiles: Vec<CcSwitchProfileRecord>,
) {
    for record in profiles {
        let key = cc_switch_profile_import_key(&record.profile);
        if seen.insert(key) {
            detected.push(DetectedCcSwitchProfile {
                source_path: path.to_string_lossy().into_owned(),
                profile: record.profile,
                api_key: record.api_key,
            });
        }
    }
}

pub(crate) fn dedupe_cc_switch_profiles(
    profiles: Vec<CcSwitchProfileRecord>,
) -> Vec<CcSwitchProfileRecord> {
    let mut seen = BTreeSet::new();
    profiles
        .into_iter()
        .filter(|record| seen.insert(cc_switch_profile_import_key(&record.profile)))
        .collect()
}

pub(crate) fn cc_switch_profile_import_key(profile: &Profile) -> String {
    format!(
        "{}|{}|{}|{}",
        cc_switch_profile_key_part(&profile.name),
        cc_switch_profile_key_part(&profile.provider),
        cc_switch_profile_key_part(&profile.model),
        cc_switch_profile_base_url_key(profile.base_url.as_deref())
    )
}

pub(crate) fn cc_switch_profile_key_part(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(crate) fn cc_switch_profile_base_url_key(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

pub(crate) fn contains_key_material(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("sk-")
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("-----begin ")
}

pub(crate) fn normalize_required_text(label: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required."))
    } else {
        Ok(value.to_string())
    }
}

pub(crate) fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn slugify(value: &str) -> String {
    let slug = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "profile".into()
    } else {
        slug
    }
}

pub(crate) fn sanitize_toml_key(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Provider is required.".into());
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        Ok(value.to_string())
    } else {
        Err("Provider may only contain ASCII letters, numbers, hyphens, and underscores.".into())
    }
}

pub(crate) fn timestamp_label() -> String {
    timestamp_millis().to_string()
}

pub(crate) fn date_label() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
}

pub(crate) fn empty_hosts() -> Vec<Host> {
    Vec::new()
}

pub(crate) fn empty_profiles() -> Vec<Profile> {
    Vec::new()
}

pub(crate) fn empty_skill_packs() -> Vec<SkillPack> {
    Vec::new()
}
