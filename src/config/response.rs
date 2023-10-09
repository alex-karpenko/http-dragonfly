use figment::Error;
use regex::Regex;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer,
};
use std::fmt::Display;

use crate::errors::HttpDragonflyError;

use super::headers::HeaderTransform;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct ResponseConfig {
    pub strategy: ResponseStrategy,
    pub target_selector: Option<String>,
    failed_status_regex: String,
    no_targets_status: ResponseStatus,
    #[serde(rename = "override")]
    pub override_config: Option<OverrideConfig>,
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            strategy: Default::default(),
            target_selector: Default::default(),
            failed_status_regex: "4\\d{2}|5\\d{2}".into(),
            no_targets_status: "500 No valid targets".into(),
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

#[derive(Debug)]
pub struct ResponseStatus {
    code: u16,
    msg: Option<String>,
}

impl<'de> Deserialize<'de> for ResponseStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ResponseStatusVisitor;
        impl<'de> Visitor<'de> for ResponseStatusVisitor {
            type Value = ResponseStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string with three-digit status code and optional status message, i.e. `200 OK`")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                ResponseStatus::from_str(v).map_err(|e| E::custom(e.to_string()))
            }
        }

        deserializer.deserialize_string(ResponseStatusVisitor)
    }
}

impl ResponseStatus {
    fn from_str(v: &str) -> Result<Self, HttpDragonflyError> {
        let re = Regex::new(r"^(?P<code>\d{3})\s*(?P<msg>.*)$")
            .unwrap_or_else(|e| panic!("looks like a BUG: {e}"));
        let caps = re.captures(v);

        if let Some(caps) = caps {
            let code: u16 =
                caps["code"]
                    .parse()
                    .map_err(|_e| HttpDragonflyError::ParseConfigFile {
                        cause: Error::from(String::from("invalid status string")),
                    })?;
            let msg: Option<String> = match &caps["msg"] {
                "" => None,
                _ => Some(caps["msg"].trim().into()),
            };

            Ok(Self { code, msg })
        } else {
            Err(HttpDragonflyError::ParseConfigFile {
                cause: Error::from(String::from("invalid status string")),
            })
        }
    }

    pub fn get_code(&self) -> u16 {
        self.code
    }
}

impl From<&str> for ResponseStatus {
    fn from(value: &str) -> Self {
        ResponseStatus::from_str(value).unwrap()
    }
}

impl Display for ResponseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(msg) = &self.msg {
            write!(f, "{} {}", self.code, msg)
        } else {
            write!(f, "{}", self.code)
        }
    }
}
