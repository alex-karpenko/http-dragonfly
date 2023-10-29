use serde::Deserialize;
use std::fmt::Display;

use super::headers::HeaderTransform;

pub type ResponseStatus = u16;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct ResponseConfig {
    pub strategy: ResponseStrategy,
    pub target_selector: Option<String>,
    pub failed_status_regex: String,
    pub no_targets_status: ResponseStatus,
    #[serde(rename = "override")]
    pub override_config: Option<OverrideConfig>,
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            strategy: Default::default(),
            target_selector: Default::default(),
            failed_status_regex: "4\\d{2}|5\\d{2}".into(),
            no_targets_status: 500,
            override_config: None,
        }
    }
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ResponseStrategy {
    AlwaysOverride,
    AlwaysTargetId,
    OkThenFailed,
    OkThenTargetId,
    OkThenOverride,
    FailedThenOk,
    FailedThenTargetId,
    #[default]
    FailedThenOverride,
    ConditionalRouting,
}

impl Display for ResponseStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ResponseStrategy::AlwaysOverride => "always_override",
                ResponseStrategy::AlwaysTargetId => "always_target_id",
                ResponseStrategy::OkThenFailed => "ok_then_failed",
                ResponseStrategy::OkThenTargetId => "ok_then_target_id",
                ResponseStrategy::OkThenOverride => "ok_then_override",
                ResponseStrategy::FailedThenOk => "failed_then_ok",
                ResponseStrategy::FailedThenTargetId => "failed_then_target_id",
                ResponseStrategy::FailedThenOverride => "failed_then_override",
                ResponseStrategy::ConditionalRouting => "conditional_routing",
            }
        )
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct OverrideConfig {
    pub status: Option<ResponseStatus>,
    pub body: Option<String>,
    pub headers: Option<Vec<HeaderTransform>>,
}
