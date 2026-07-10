use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "AppUpdateStateDto")]
pub(crate) enum AppUpdateState {
    Disabled,
    PendingConfiguration,
    Ready,
    UpToDate,
    Available,
    Installing,
    Error,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "AppUpdateStatusDto")]
pub(crate) struct AppUpdateStatus {
    pub(crate) software_name: String,
    pub(crate) channel: String,
    pub(crate) current_version: String,
    pub(crate) installed_at: Option<String>,
    pub(crate) state: AppUpdateState,
    pub(crate) configured: bool,
    pub(crate) feed_configured: bool,
    pub(crate) signing_configured: bool,
    pub(crate) latest_version: Option<String>,
    pub(crate) checked_at: Option<String>,
    pub(crate) message: String,
}

#[derive(Clone)]
pub(crate) struct StableUpdaterConfig {
    pub(crate) endpoint: Option<String>,
    pub(crate) pubkey: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GitHubReleaseAsset {
    pub(crate) name: String,
    pub(crate) url: String,
}

#[derive(Deserialize)]
pub(crate) struct GitHubReleaseResponse {
    pub(crate) assets: Vec<GitHubReleaseAsset>,
}
