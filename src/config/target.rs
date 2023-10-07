use hyper::Uri;
use serde::Deserialize;
use std::time::Duration;

use crate::errors::HttpDragonflyError;

use super::headers::HeaderTransform;

const DEFAULT_TARGET_TIMEOUT_SEC: u64 = 60;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    id: Option<String>,
    pub url: String,
    pub headers: Option<Vec<HeaderTransform>>,
    pub body: Option<String>,
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

    pub fn get_id(&self) -> String {
        if let Some(id) = &self.id {
            id.clone()
        } else {
            format!("TARGET-{}", self.url)
        }
    }

    pub fn get_uri(&self) -> Result<Uri, HttpDragonflyError> {
        self.url
            .parse()
            .map_err(|e| HttpDragonflyError::InvalidConfig {
                cause: format!("invalid url `{}`: {e}", self.url),
            })
    }
}
