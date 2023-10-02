use figment::{
    providers::{Format, Yaml},
    Error, Figment,
};
use regex::Regex;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{fmt::Display, fs::read_to_string, time::Duration};
use tracing::debug;

use crate::errors::HttpSplitterError;

const DEFAULT_TARGET_TIMEOUT_SEC: u64 = 60;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SplitterConfig {
    pub listeners: Vec<ListenerConfig>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ListenerConfig {
    name: String,
    #[serde(rename = "on")]
    listen_on: String,
    headers: Option<Vec<HeaderTransform>>,
    methods: Option<Vec<String>>,
    targets: Vec<TargetConfig>,
    response: ResponseConfig,
}

#[derive(Debug)]
struct HeaderTransform {
    action: HeaderTransformActon,
    value: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
enum HeaderTransformActon {
    Add(String),
    Replace(String),
    Drop(String),
}

impl<'de> Deserialize<'de> for HeaderTransform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        #[serde(deny_unknown_fields, rename_all = "lowercase")]
        enum Fields {
            Drop,
            Add,
            Replace,
            Value,
        }

        struct HeaderTransformVisitor;
        impl<'de> Visitor<'de> for HeaderTransformVisitor {
            type Value = HeaderTransform;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct HeaderTransform")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut action: Option<HeaderTransformActon> = None;
                let mut value: Option<String> = None;

                // Extract all fields
                while let Some(key) = map.next_key()? {
                    match key {
                        Fields::Add => {
                            if action.is_some() {
                                return Err(de::Error::duplicate_field("add"));
                            }
                            action = Some(HeaderTransformActon::Add(map.next_value::<String>()?))
                        }
                        Fields::Drop => {
                            if action.is_some() {
                                return Err(de::Error::duplicate_field("drop"));
                            }
                            action = Some(HeaderTransformActon::Drop(map.next_value::<String>()?))
                        }
                        Fields::Replace => {
                            if action.is_some() {
                                return Err(de::Error::duplicate_field("replace"));
                            }
                            action =
                                Some(HeaderTransformActon::Replace(map.next_value::<String>()?))
                        }
                        Fields::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value::<String>()?)
                        }
                    }
                }

                if let Some(action) = action {
                    match action {
                        HeaderTransformActon::Add(_) | HeaderTransformActon::Replace(_) => {
                            if value.is_none() {
                                return Err(de::Error::missing_field("value"));
                            }
                        }
                        HeaderTransformActon::Drop(_) => {
                            if value.is_some() {
                                return Err(de::Error::custom(
                                    "unknown field value in action drop",
                                ));
                            }
                        }
                    }
                    Ok(HeaderTransform { action, value })
                } else {
                    return Err(de::Error::missing_field(
                        "action should be one of add/drop/replace",
                    ));
                }
            }
        }

        const FIELDS: &'static [&'static str] = &["add", "drop", "replace", "value"];
        deserializer.deserialize_struct("HeaderAction", FIELDS, HeaderTransformVisitor)
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TargetConfig {
    id: String,
    url: String,
    headers: Option<Vec<HeaderTransform>>,
    #[serde(default = "TargetConfig::default_body")]
    body: String,
    #[serde(
        with = "humantime_serde",
        default = "TargetConfig::default_target_timeout"
    )]
    timeout: Duration,
}

impl TargetConfig {
    fn default_target_timeout() -> Duration {
        Duration::from_secs(DEFAULT_TARGET_TIMEOUT_SEC)
    }

    fn default_body() -> String {
        "${REQUEST_BODY}".into()
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ResponseConfig {
    strategy: ResponseStrategy,
    target_selector: ResponseTargetSelector,
    failure_status_regex: String,
    failure_on_timeout: bool,
    #[serde(default = "ResponseConfig::default_timeout_status")]
    timeout_status: ResponseStatus,
    cancel_unneeded_targets: bool,
    #[serde(rename = "override")]
    override_config: Option<OverrideConfig>,
}

impl ResponseConfig {
    fn default_timeout_status() -> ResponseStatus {
        String::from("504 Gateway Timeout").into()
    }
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
enum ResponseStrategy {
    AlwaysOverride,
    AlwaysTargetId,
    OkThenFailed,
    OkThenTargetId,
    OkThenOverride,
    FailedThenOk,
    FailedThenTargetId,
    #[default]
    FailedThenOverride,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
enum ResponseTargetSelector {
    #[default]
    Fastest,
    Slowest,
    Random,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct OverrideConfig {
    status: Option<ResponseStatus>,
    body: Option<String>,
    headers: Option<Vec<HeaderTransform>>,
}

#[derive(Debug)]
struct ResponseStatus {
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

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                ResponseStatus::from_string(v).map_err(|e| E::custom(e.to_string()))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_string(v.into())
            }
        }

        deserializer.deserialize_string(ResponseStatusVisitor)
    }
}

impl ResponseStatus {
    fn from_string(v: String) -> Result<Self, HttpSplitterError> {
        let re = Regex::new(r"^(?P<code>\d{3})\s*(?P<msg>.*)$")
            .unwrap_or_else(|e| panic!("looks like a BUG: {e}"));
        let caps = re.captures(&v);

        if let Some(caps) = caps {
            let code: u16 =
                caps["code"]
                    .parse()
                    .map_err(|_e| HttpSplitterError::ParseConfigFile {
                        cause: Error::from(String::from("invalid status string")),
                    })?;
            let msg: Option<String> = match &caps["msg"] {
                "" => None,
                _ => Some(caps["msg"].trim().into()),
            };

            Ok(Self { code, msg })
        } else {
            Err(HttpSplitterError::ParseConfigFile {
                cause: Error::from(String::from("invalid status string")),
            })
        }
    }
}

impl From<String> for ResponseStatus {
    fn from(value: String) -> Self {
        ResponseStatus::from_string(value).unwrap()
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

impl SplitterConfig {
    pub fn from(filename: &String) -> Result<SplitterConfig, HttpSplitterError> {
        let config = read_to_string(filename).map_err(|e| HttpSplitterError::LoadConfigFile {
            filename: filename.clone(),
            cause: e,
        })?;
        let config: SplitterConfig = Figment::new().merge(Yaml::string(&config)).extract()?;
        debug!("{:#?}", config);
        Ok(config)
    }

    pub fn len(&self) -> usize {
        self.listeners.len()
    }
}
