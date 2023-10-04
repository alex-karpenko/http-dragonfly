use std::time::Duration;
use serde::Deserialize;

use super::{
    headers::HeaderTransform,
    listener::{ListenOn, DEFAULT_LISTENER_TIMEOUT_SEC},
    response::ResponseConfig,
    target::TargetConfig,
};

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SplitterListenerConfig {
    name: Option<String>,
    #[serde(rename = "on", default)]
    listen_on: ListenOn,
    #[serde(
        with = "humantime_serde",
        default = "SplitterListenerConfig::default_listener_timeout"
    )]
    timeout: Duration,
    headers: Option<Vec<HeaderTransform>>,
    methods: Option<Vec<String>>,
    targets: Vec<TargetConfig>,
    #[serde(default)]
    response: ResponseConfig,
}

impl SplitterListenerConfig {
    fn default_listener_timeout() -> Duration {
        Duration::from_secs(DEFAULT_LISTENER_TIMEOUT_SEC)
    }

    pub fn get_name(self) -> String {
        if let Some(name) = self.name {
            name
        } else {
            format!("LISTENER-{}", self.listen_on)
        }
    }
}
