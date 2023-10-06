use serde::Deserialize;
use std::time::Duration;

use super::headers::HeaderTransform;

const DEFAULT_TARGET_TIMEOUT_SEC: u64 = 60;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    id: Option<String>,
    url: String,
    headers: Option<Vec<HeaderTransform>>,
    #[serde(default = "TargetConfig::default_body")]
    body: String,
    #[serde(
        with = "humantime_serde",
        default = "TargetConfig::default_target_timeout"
    )]
    timeout: Duration,
    pub condition: Option<String>,
}

impl TargetConfig {
    fn default_target_timeout() -> Duration {
        Duration::from_secs(DEFAULT_TARGET_TIMEOUT_SEC)
    }

    fn default_body() -> String {
        "${REQUEST_BODY}".into()
    }

    pub fn get_id(self) -> String {
        if let Some(id) = self.id {
            id
        } else {
            format!("TARGET-{}", self.url)
        }
    }
}
