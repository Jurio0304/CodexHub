use crate::*;

pub(crate) fn app_health() -> Health {
    Health {
        app: "CodexHub",
        mode: "tauri",
        remote_wrapper_required: false,
    }
}

pub(crate) fn get_app_update_status(app: AppHandle) -> AppUpdateStatus {
    app_update_status_for_channel(
        current_app_channel(&app),
        current_app_version(&app),
        None,
        None,
    )
}

pub(crate) async fn check_stable_update(
    app: AppHandle,
    state: &AppState,
) -> Result<AppUpdateStatus, String> {
    ensure_task_storage_healthy(&state)?;
    let task_id = format!("task-app-update-check-{}", timestamp_millis());
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        "local-app",
        &app_display_name(&app),
        "Check app update",
    )?;
    let settings = read_settings(&state.paths)?;
    let (status, attempts) = check_stable_update_status(app, &settings).await;
    record_task(&state, app_update_check_task(running, &status, &attempts))?;
    Ok(status)
}

pub(crate) async fn check_stable_update_status(
    app: AppHandle,
    settings: &AppSettings,
) -> (AppUpdateStatus, Vec<String>) {
    let channel = current_app_channel(&app);
    let current_version = current_app_version(&app);
    if channel != "stable" {
        return (
            app_update_status_for_channel(
                channel,
                current_version,
                Some(AppUpdateState::Disabled),
                None,
            ),
            Vec::new(),
        );
    }

    let config = stable_updater_config();
    if !stable_updater_configured(&config) {
        return (
            app_update_status_for_channel(
                channel,
                current_version,
                Some(AppUpdateState::PendingConfiguration),
                None,
            ),
            Vec::new(),
        );
    }

    let endpoint = match config
        .endpoint
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
    {
        Some(endpoint) => endpoint,
        None => {
            return (
                app_update_status(
                    channel,
                    &current_version,
                    AppUpdateState::Error,
                    &config,
                    None,
                    Some(update_checked_at()),
                    "Stable updater endpoint is configured but invalid. Rebuild with a valid HTTPS feed URL.".into(),
                ),
                Vec::new(),
            );
        }
    };
    if endpoint.scheme() != "https" {
        return (
            app_update_status(
                channel,
                &current_version,
                AppUpdateState::Error,
                &config,
                None,
                Some(update_checked_at()),
                "Stable updater endpoint must use HTTPS.".into(),
            ),
            Vec::new(),
        );
    }

    let pubkey = config.pubkey.clone().unwrap_or_default();
    let (routes, route_notes) = stable_update_network_routes(settings);
    let mut attempts = route_notes;
    let mut last_error = None;

    for route in routes {
        let label = route.label();
        let endpoints = stable_update_endpoints(&endpoint, route.proxy.as_ref()).await;
        let updater = match stable_updater(&app, pubkey.clone(), endpoints, route.proxy.clone()) {
            Ok(updater) => updater,
            Err(error) => {
                let message = updater_error_message(
                    "Stable updater could not initialize",
                    error,
                    "Verify the release feed and public signing key.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
                continue;
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                attempts.push(format!("{label}: update {} is available", update.version));
                return (
                    app_update_status(
                        channel,
                        &current_version,
                        AppUpdateState::Available,
                        &config,
                        Some(update.version),
                        Some(update_checked_at()),
                        format!("A signed stable update is available via {label}. Use Install update to let Windows apply it."),
                    ),
                    attempts,
                );
            }
            Ok(None) => {
                attempts.push(format!("{label}: CodexHub stable is up to date"));
                return (
                    app_update_status(
                        channel,
                        &current_version,
                        AppUpdateState::UpToDate,
                        &config,
                        None,
                        Some(update_checked_at()),
                        format!("CodexHub stable is up to date via {label}."),
                    ),
                    attempts,
                );
            }
            Err(error) => {
                let message = updater_error_message(
                    "Stable update check failed",
                    error,
                    "Verify the configured feed, signatures, and network path.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
            }
        }
    }

    (
        app_update_status(
            channel,
            &current_version,
            AppUpdateState::Error,
            &config,
            None,
            Some(update_checked_at()),
            format!(
                "Stable update check failed across all network routes. {}",
                last_error.unwrap_or_else(|| "No updater route was available.".into())
            ),
        ),
        attempts,
    )
}

pub(crate) async fn install_stable_update(
    app: AppHandle,
    state: &AppState,
) -> Result<AppUpdateStatus, String> {
    ensure_task_storage_healthy(&state)?;
    let channel = current_app_channel(&app);
    let current_version = current_app_version(&app);
    let task_id = format!("task-app-update-install-{}", timestamp_millis());
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        "local-app",
        &app_display_name(&app),
        "Install app update",
    )?;
    if channel != "stable" {
        let status = app_update_status_for_channel(
            channel,
            current_version,
            Some(AppUpdateState::Disabled),
            None,
        );
        record_task(&state, app_update_install_task(running, &status, &[]))?;
        return Ok(status);
    }

    let config = stable_updater_config();
    if !stable_updater_configured(&config) {
        let status = app_update_status_for_channel(
            channel,
            current_version,
            Some(AppUpdateState::PendingConfiguration),
            None,
        );
        record_task(&state, app_update_install_task(running, &status, &[]))?;
        return Ok(status);
    }

    let endpoint = match config
        .endpoint
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
    {
        Some(endpoint) => endpoint,
        None => {
            let status = app_update_status(
                channel,
                &current_version,
                AppUpdateState::Error,
                &config,
                None,
                Some(update_checked_at()),
                "Stable updater endpoint is configured but invalid. Rebuild with a valid HTTPS feed URL.".into(),
            );
            record_task(&state, app_update_install_task(running, &status, &[]))?;
            return Ok(status);
        }
    };
    if endpoint.scheme() != "https" {
        let status = app_update_status(
            channel,
            &current_version,
            AppUpdateState::Error,
            &config,
            None,
            Some(update_checked_at()),
            "Stable updater endpoint must use HTTPS.".into(),
        );
        record_task(&state, app_update_install_task(running, &status, &[]))?;
        return Ok(status);
    }

    let pubkey = config.pubkey.clone().unwrap_or_default();
    let settings = read_settings(&state.paths)?;
    let (routes, route_notes) = stable_update_network_routes(&settings);
    let mut attempts = route_notes;
    let mut last_error = None;

    for route in routes {
        let label = route.label();
        let endpoints = stable_update_endpoints(&endpoint, route.proxy.as_ref()).await;
        let updater = match stable_updater(&app, pubkey.clone(), endpoints, route.proxy.clone()) {
            Ok(updater) => updater,
            Err(error) => {
                let message = updater_error_message(
                    "Stable updater could not initialize",
                    error,
                    "Verify the release feed and public signing key.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
                continue;
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                let latest_version = update.version.clone();
                attempts.push(format!("{label}: downloading update {latest_version}"));
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(()) => {
                        attempts.push(format!("{label}: installer started"));
                        let status = app_update_status(
                            channel,
                            &current_version,
                            AppUpdateState::Installing,
                            &config,
                            Some(latest_version),
                            Some(update_checked_at()),
                            format!("Stable update installer started via {label}. CodexHub will close while Windows applies the update."),
                        );
                        record_task(&state, app_update_install_task(running, &status, &attempts))?;
                        return Ok(status);
                    }
                    Err(error) => {
                        let message = updater_error_message(
                            "Stable update install failed",
                            error,
                            "Verify the signed artifact, feed metadata, installer path, and proxy route.",
                        );
                        attempts.push(format!("{label}: {message}"));
                        last_error = Some(message);
                    }
                }
            }
            Ok(None) => {
                attempts.push(format!("{label}: CodexHub stable is up to date"));
                let status = app_update_status(
                    channel,
                    &current_version,
                    AppUpdateState::UpToDate,
                    &config,
                    None,
                    Some(update_checked_at()),
                    format!("CodexHub stable is up to date via {label}."),
                );
                record_task(&state, app_update_install_task(running, &status, &attempts))?;
                return Ok(status);
            }
            Err(error) => {
                let message = updater_error_message(
                    "Stable update check failed",
                    error,
                    "Verify the configured feed, signatures, and network path.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
            }
        }
    }

    let status = app_update_status(
        channel,
        &current_version,
        AppUpdateState::Error,
        &config,
        None,
        Some(update_checked_at()),
        format!(
            "Stable update install failed across all network routes. {}",
            last_error.unwrap_or_else(|| "No updater route was available.".into())
        ),
    );
    record_task(&state, app_update_install_task(running, &status, &attempts))?;
    Ok(status)
}

pub(crate) fn current_app_channel(app: &AppHandle) -> &'static str {
    match app.config().identifier.as_str() {
        STABLE_IDENTIFIER => "stable",
        DEV_IDENTIFIER => "dev",
        _ => "dev",
    }
}

pub(crate) fn current_app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

pub(crate) fn stable_updater_config() -> StableUpdaterConfig {
    StableUpdaterConfig {
        endpoint: non_empty_compile_env(option_env!("CODEXHUB_STABLE_UPDATE_ENDPOINT")),
        pubkey: option_env!("CODEXHUB_STABLE_UPDATER_PUBKEY").and_then(normalize_updater_pubkey),
    }
}

pub(crate) fn non_empty_compile_env(value: Option<&'static str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn normalize_updater_pubkey(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(bytes) = general_purpose::STANDARD.decode(trimmed) {
        if let Ok(decoded) = String::from_utf8(bytes) {
            if decoded.contains("minisign public key") {
                if let Some(pub_file) = normalize_minisign_pub_file_text(&decoded) {
                    return Some(general_purpose::STANDARD.encode(pub_file.as_bytes()));
                }
            }
        }
        if let Some(pub_file) = minisign_pub_file_from_key_line(trimmed) {
            return Some(general_purpose::STANDARD.encode(pub_file.as_bytes()));
        }
    }
    if trimmed.contains("minisign public key") || trimmed.contains('\n') {
        if let Some(pub_file) = normalize_minisign_pub_file_text(trimmed) {
            return Some(general_purpose::STANDARD.encode(pub_file.as_bytes()));
        }
    }
    None
}

pub(crate) fn normalize_minisign_pub_file_text(value: &str) -> Option<String> {
    let key_line = extract_minisign_public_key_line(value)?;
    let comment = value
        .lines()
        .map(str::trim)
        .find(|line| line.contains("minisign public key"))
        .map(ToOwned::to_owned)
        .or_else(|| {
            minisign_key_id(&key_line)
                .map(|key_id| format!("untrusted comment: minisign public key: {key_id}"))
        })?;
    Some(format!("{comment}\n{key_line}\n"))
}

pub(crate) fn extract_minisign_public_key_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| minisign_key_id(line).is_some())
        .map(ToOwned::to_owned)
}

pub(crate) fn minisign_pub_file_from_key_line(value: &str) -> Option<String> {
    let key_id = minisign_key_id(value)?;
    Some(format!(
        "untrusted comment: minisign public key: {key_id}\n{value}\n"
    ))
}

pub(crate) fn minisign_key_id(value: &str) -> Option<String> {
    let bytes = general_purpose::STANDARD.decode(value).ok()?;
    if bytes.len() != 42 {
        return None;
    }
    if bytes.first() != Some(&0x45) || !matches!(bytes.get(1).copied(), Some(0x64 | 0x44)) {
        return None;
    }
    Some(
        bytes[2..10]
            .iter()
            .rev()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(""),
    )
}

pub(crate) fn stable_updater_configured(config: &StableUpdaterConfig) -> bool {
    config.endpoint.is_some() && config.pubkey.is_some()
}

#[derive(Clone)]
pub(crate) struct StableUpdateNetworkRoute {
    source: String,
    proxy: Option<Url>,
}

impl StableUpdateNetworkRoute {
    fn direct() -> Self {
        Self {
            source: "direct".into(),
            proxy: None,
        }
    }

    fn label(&self) -> String {
        match &self.proxy {
            Some(proxy) => format!("{} {}", self.source, redact_proxy_url(proxy)),
            None => self.source.clone(),
        }
    }
}

const LOCAL_PROXY_PORTS: &[u16] = &[7890, 7897, 7891, 1080, 10808, 8080, 9090, 20171];
const PROXY_ENV_NAMES: &[&str] = &[
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

pub(crate) fn normalize_proxy_url(value: &str) -> Option<Url> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(port) = trimmed.parse::<u16>() {
        return Url::parse(&format!("http://127.0.0.1:{port}")).ok();
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };
    let url = Url::parse(&candidate).ok()?;
    match url.scheme() {
        "http" | "https" | "socks4" | "socks5" | "socks5h" => Some(url),
        _ => None,
    }
}

pub(crate) fn redact_proxy_url(url: &Url) -> String {
    let mut redacted = url.clone();
    if !redacted.username().is_empty() {
        if redacted.set_username("redacted").is_err() {
            return "[redacted proxy URL]".into();
        }
    }
    if redacted.password().is_some() {
        if redacted.set_password(Some("redacted")).is_err() {
            return "[redacted proxy URL]".into();
        }
    }
    redacted.to_string()
}

pub(crate) fn proxy_is_localhost(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

pub(crate) fn localhost_proxy_port_is_open(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(180)).is_ok()
}

pub(crate) fn proxy_candidate(source: String, url: Url) -> NetworkProxyCandidate {
    let available = if proxy_is_localhost(&url) {
        url.port()
            .map(localhost_proxy_port_is_open)
            .unwrap_or(false)
    } else {
        true
    };
    let message = if available {
        "Proxy route is available for updater retry.".into()
    } else {
        "Local proxy port did not accept a TCP connection.".into()
    };
    NetworkProxyCandidate {
        source,
        url: Some(redact_proxy_url(&url)),
        available,
        message,
    }
}

pub(crate) fn env_proxy_candidates() -> Vec<(String, Url)> {
    let mut entries = Vec::new();
    for name in PROXY_ENV_NAMES {
        if let Ok(value) = env::var(name) {
            if let Some(url) = normalize_proxy_url(&value) {
                entries.push((format!("env:{name}"), url));
            }
        }
    }
    dedupe_proxy_entries(entries)
}

pub(crate) fn local_proxy_candidates() -> Vec<(String, Url)> {
    LOCAL_PROXY_PORTS
        .iter()
        .filter(|port| localhost_proxy_port_is_open(**port))
        .filter_map(|port| {
            normalize_proxy_url(&format!("http://127.0.0.1:{port}"))
                .map(|url| (format!("local-port:{port}"), url))
        })
        .collect()
}

pub(crate) fn dedupe_proxy_entries(entries: Vec<(String, Url)>) -> Vec<(String, Url)> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for (source, url) in entries {
        let key = url.to_string();
        if seen.insert(key) {
            deduped.push((source, url));
        }
    }
    deduped
}

pub(crate) fn stable_update_network_routes(
    settings: &AppSettings,
) -> (Vec<StableUpdateNetworkRoute>, Vec<String>) {
    let mut routes = vec![StableUpdateNetworkRoute::direct()];
    let mut notes = Vec::new();
    let mut seen = BTreeSet::from(["direct".to_string()]);
    let mut push_proxy = |routes: &mut Vec<StableUpdateNetworkRoute>, source: String, url: Url| {
        let key = url.to_string();
        if seen.insert(key) {
            routes.push(StableUpdateNetworkRoute {
                source,
                proxy: Some(url),
            });
        }
    };

    match settings.network_proxy_mode {
        NetworkProxyMode::Direct => {}
        NetworkProxyMode::Manual => {
            if let Some(url) = normalize_proxy_url(&settings.network_proxy_url) {
                push_proxy(&mut routes, "manual".into(), url);
            } else if !settings.network_proxy_url.trim().is_empty() {
                notes.push("manual proxy URL is invalid".into());
            }
        }
        NetworkProxyMode::Auto => {
            for (source, url) in env_proxy_candidates() {
                push_proxy(&mut routes, source, url);
            }
            for (source, url) in local_proxy_candidates() {
                push_proxy(&mut routes, source, url);
            }
        }
    }

    (routes, notes)
}

pub(crate) fn detect_network_proxy_status(settings: &AppSettings) -> NetworkProxyStatus {
    if settings.network_proxy_mode == NetworkProxyMode::Direct {
        return NetworkProxyStatus {
            mode: settings.network_proxy_mode.clone(),
            proxy_url: None,
            source: None,
            message: "Network proxy is disabled; stable updater will use direct connections only."
                .into(),
            candidates: Vec::new(),
        };
    }

    let mut candidates = Vec::new();
    if let Some(manual) = normalize_proxy_url(&settings.network_proxy_url) {
        candidates.push(proxy_candidate("manual".into(), manual));
    } else if !settings.network_proxy_url.trim().is_empty() {
        candidates.push(NetworkProxyCandidate {
            source: "manual".into(),
            url: None,
            available: false,
            message: "Manual proxy URL is invalid.".into(),
        });
    }
    for (source, url) in env_proxy_candidates() {
        candidates.push(proxy_candidate(source, url));
    }
    for port in LOCAL_PROXY_PORTS {
        let url =
            normalize_proxy_url(&format!("http://127.0.0.1:{port}")).expect("local proxy URL");
        candidates.push(proxy_candidate(format!("local-port:{port}"), url));
    }

    let selected = candidates
        .iter()
        .find(|candidate| candidate.available && candidate.url.is_some());
    NetworkProxyStatus {
        mode: settings.network_proxy_mode.clone(),
        proxy_url: selected.and_then(|candidate| candidate.url.clone()),
        source: selected.map(|candidate| candidate.source.clone()),
        message: selected
            .map(|candidate| format!("Detected updater proxy route from {}.", candidate.source))
            .unwrap_or_else(|| "No local proxy port is currently reachable.".into()),
        candidates,
    }
}

pub(crate) async fn stable_update_endpoints(endpoint: &Url, proxy: Option<&Url>) -> Vec<Url> {
    let mut endpoints = Vec::new();
    if let Some(github_asset_endpoint) =
        resolve_github_latest_json_asset_endpoint(endpoint, proxy).await
    {
        endpoints.push(github_asset_endpoint);
    }
    endpoints.push(endpoint.clone());
    endpoints.dedup();
    endpoints
}

pub(crate) fn stable_updater(
    app: &AppHandle,
    pubkey: String,
    endpoints: Vec<Url>,
    proxy: Option<Url>,
) -> std::result::Result<tauri_plugin_updater::Updater, tauri_plugin_updater::Error> {
    let use_asset_api = endpoints.iter().any(is_github_release_asset_api_endpoint);
    let mut builder = app
        .updater_builder()
        .pubkey(pubkey)
        .endpoints(endpoints)?
        .timeout(Duration::from_secs(APP_UPDATE_CHECK_TIMEOUT_SECS));
    if let Some(proxy) = proxy {
        builder = builder.proxy(proxy);
    }
    if use_asset_api {
        builder = builder.header("Accept", OCTET_STREAM_ACCEPT)?;
    }
    builder.build()
}

pub(crate) async fn resolve_github_latest_json_asset_endpoint(
    endpoint: &Url,
    proxy: Option<&Url>,
) -> Option<Url> {
    let api_url = github_release_api_url(endpoint)?;
    let mut client_builder = reqwest::Client::builder()
        .user_agent("CodexHub updater feed resolver")
        .timeout(Duration::from_secs(APP_UPDATE_CHECK_TIMEOUT_SECS));
    if let Some(proxy) = proxy {
        client_builder = client_builder.proxy(reqwest::Proxy::all(proxy.as_str()).ok()?);
    }
    let client = client_builder.build().ok()?;
    let response = client
        .get(api_url)
        .header("Accept", GITHUB_API_ACCEPT)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let release = response.json::<GitHubReleaseResponse>().await.ok()?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == "latest.json")?;
    Url::parse(&asset.url).ok()
}

pub(crate) fn github_release_api_url(endpoint: &Url) -> Option<String> {
    if endpoint.scheme() != "https" || endpoint.host_str() != Some("github.com") {
        return None;
    }
    let segments = endpoint.path_segments()?.collect::<Vec<_>>();
    match segments.as_slice() {
        [owner, repo, "releases", "latest", "download", "latest.json"] => Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/releases/latest"
        )),
        [owner, repo, "releases", "download", tag, "latest.json"] => Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}"
        )),
        _ => None,
    }
}

pub(crate) fn is_github_release_asset_api_endpoint(endpoint: &Url) -> bool {
    endpoint.scheme() == "https"
        && endpoint.host_str() == Some("api.github.com")
        && endpoint.path().contains("/releases/assets/")
}

pub(crate) fn app_update_status_for_channel(
    channel: &'static str,
    current_version: String,
    state_override: Option<AppUpdateState>,
    latest_version: Option<String>,
) -> AppUpdateStatus {
    let config = stable_updater_config();
    let state = state_override.unwrap_or_else(|| {
        if channel != "stable" {
            AppUpdateState::Disabled
        } else if stable_updater_configured(&config) {
            AppUpdateState::Ready
        } else {
            AppUpdateState::PendingConfiguration
        }
    });
    let message = app_update_message(channel, &state);
    app_update_status(
        channel,
        &current_version,
        state,
        &config,
        latest_version,
        None,
        message,
    )
}

pub(crate) fn app_update_status(
    channel: &'static str,
    current_version: &str,
    state: AppUpdateState,
    config: &StableUpdaterConfig,
    latest_version: Option<String>,
    checked_at: Option<String>,
    message: String,
) -> AppUpdateStatus {
    let latest_version = match state {
        AppUpdateState::UpToDate => latest_version.or_else(|| Some(current_version.into())),
        _ => latest_version,
    };

    AppUpdateStatus {
        software_name: app_name_for_channel(channel).into(),
        channel: channel.into(),
        current_version: current_version.into(),
        installed_at: current_app_installed_at(),
        configured: stable_updater_configured(config),
        feed_configured: config.endpoint.is_some(),
        signing_configured: config.pubkey.is_some(),
        latest_version,
        checked_at,
        state,
        message,
    }
}

pub(crate) fn app_update_check_task(
    running: TaskRun,
    status: &AppUpdateStatus,
    attempts: &[String],
) -> TaskRun {
    app_update_task(running, "check_stable_update", status, attempts)
}

pub(crate) fn app_update_install_task(
    running: TaskRun,
    status: &AppUpdateStatus,
    attempts: &[String],
) -> TaskRun {
    app_update_task(running, "install_stable_update", status, attempts)
}

pub(crate) fn app_update_task(
    mut task: TaskRun,
    command: &str,
    status: &AppUpdateStatus,
    attempts: &[String],
) -> TaskRun {
    let task_id = task.id.clone();
    let failed = matches!(&status.state, AppUpdateState::Error);
    let log_level = match &status.state {
        AppUpdateState::Error => TaskLogLevel::Error,
        AppUpdateState::Disabled | AppUpdateState::PendingConfiguration => TaskLogLevel::Warn,
        _ => TaskLogLevel::Info,
    };
    let latest_version = status.latest_version.as_deref().unwrap_or("not checked");
    let checked_at = status.checked_at.as_deref().unwrap_or("not checked");
    let task_time = status.checked_at.clone().unwrap_or_else(update_checked_at);
    let attempt_details = if attempts.is_empty() {
        "networkRoutes: no route attempts recorded".into()
    } else {
        format!("networkRoutes:\n{}", attempts.join("\n"))
    };
    let details = format!(
        "softwareName: {}\nchannel: {}\ncurrentVersion: {}\nlatestVersion: {}\nstate: {}\ncheckedAt: {}\nfeedConfigured: {}\nsigningConfigured: {}\n{}",
        status.software_name,
        status.channel,
        status.current_version,
        latest_version,
        app_update_state_label(&status.state),
        checked_at,
        status.feed_configured,
        status.signing_configured,
        attempt_details
    );

    task.host_name = status.software_name.clone();
    task.status = if failed {
        TaskStatus::Failed
    } else {
        TaskStatus::Success
    };
    task.ended_at = Some(task_time.clone());
    task.summary = status.message.clone();
    task.logs.push(TaskLog {
        id: format!("{task_id}-log-{}", task.logs.len() + 1),
        task_run_id: task_id,
        level: log_level,
        timestamp: task_time,
        message: status.message.clone(),
        command: Some(command.into()),
        stdout: Some(details),
        stderr: if failed {
            Some(status.message.clone())
        } else {
            Some(String::new())
        },
        exit_code: Some(if failed { 1 } else { 0 }),
        duration_ms: None,
        timed_out: Some(false),
    });
    task
}

pub(crate) fn app_update_state_label(state: &AppUpdateState) -> &'static str {
    match state {
        AppUpdateState::Disabled => "disabled",
        AppUpdateState::PendingConfiguration => "pending-configuration",
        AppUpdateState::Ready => "ready",
        AppUpdateState::UpToDate => "up-to-date",
        AppUpdateState::Available => "available",
        AppUpdateState::Installing => "installing",
        AppUpdateState::Error => "error",
    }
}

pub(crate) fn app_name_for_channel(channel: &'static str) -> &'static str {
    match channel {
        "dev" => "CodexHub Dev",
        _ => "CodexHub",
    }
}

pub(crate) fn app_update_message(channel: &'static str, state: &AppUpdateState) -> String {
    match (channel, state) {
        ("dev", _) => "Dev channel auto-updates are disabled. Use local builds, preview packages, or test artifacts.".into(),
        ("stable", AppUpdateState::PendingConfiguration) => format!(
            "Stable updater is pending configuration. Set {STABLE_UPDATE_ENDPOINT_ENV} and {STABLE_UPDATER_PUBKEY_ENV} during the signed stable release build."
        ),
        ("stable", AppUpdateState::Ready) => "Stable updater feed and public key are configured. Run a manual check when ready.".into(),
        ("stable", AppUpdateState::UpToDate) => "CodexHub stable is up to date.".into(),
        ("stable", AppUpdateState::Available) => {
            "A signed stable update is available. Use Install update to let Windows apply it.".into()
        }
        ("stable", AppUpdateState::Installing) => {
            "Stable update installer started. CodexHub will close while Windows applies the update.".into()
        }
        ("stable", AppUpdateState::Error) => "Stable update check failed. Verify the configured feed, signatures, and network path.".into(),
        _ => "Stable updater status is unknown.".into(),
    }
}

pub(crate) fn updater_error_message(prefix: &str, error: impl Display, guidance: &str) -> String {
    format!("{prefix}: {error}. {guidance}")
}

pub(crate) fn update_checked_at() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(crate) fn current_app_installed_at() -> Option<String> {
    env::current_exe()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|metadata| metadata.created().or_else(|_| metadata.modified()).ok())
        .map(format_system_time)
}

pub(crate) fn format_system_time(time: SystemTime) -> String {
    let local: DateTime<Local> = time.into();
    local.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(crate) async fn run_blocking_command<T, F>(label: &'static str, command: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(command)
        .await
        .map_err(|error| format!("{label} worker failed: {error}"))
}
