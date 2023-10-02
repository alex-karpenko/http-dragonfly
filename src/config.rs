use figment::{
    providers::{Format, Yaml},
    Error, Figment,
};
use regex::Regex;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{fmt::Display, fs::read_to_string, net::Ipv4Addr, str::FromStr, time::Duration};
use tracing::debug;

use crate::errors::HttpSplitterError;

const DEFAULT_LISTENER_PORT: u16 = 8080;
const DEFAULT_TARGET_TIMEOUT_SEC: u64 = 60;
const DEFAULT_LISTENER_TIMEOUT_SEC: u64 = 10;
const INVALID_IP_ADDRESS_ERROR: &str = "IP address isn't valid";

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub splitters: Vec<SplitterListenerConfig>,
}

#[derive(Deserialize, Debug, Clone)]
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

#[derive(Debug, Clone)]
struct ListenOn {
    ip: Ipv4Addr,
    port: u16,
}

impl Default for ListenOn {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::new(0, 0, 0, 0),
            port: DEFAULT_LISTENER_PORT,
        }
    }
}

impl ListenOn {
    fn parse_ip_address(ip: &str) -> Result<Ipv4Addr, String> {
        Ipv4Addr::from_str(ip).map_err(|_| String::from(INVALID_IP_ADDRESS_ERROR))
    }

    fn from_str(v: &str) -> Result<Self, String> {
        let splitted: Vec<_> = v.trim().split(':').collect();

        if splitted.len() == 1 {
            let port: u16 = splitted[0]
                .parse()
                .map_err(|e| format!("invalid port value `{}`: {e}", splitted[0]))?;
            let ip = Ipv4Addr::new(0, 0, 0, 0);

            Ok(ListenOn { ip, port })
        } else if splitted.len() == 2 {
            let port: u16 = splitted[1]
                .parse()
                .map_err(|e| format!("invalid port value `{}`: {e}", splitted[1]))?;

            let ip = if splitted[0].is_empty() || splitted[0] == "*" {
                Ipv4Addr::new(0, 0, 0, 0)
            } else {
                Self::parse_ip_address(splitted[0])?
            };

            Ok(ListenOn { ip, port })
        } else {
            Err("invalid `listen on` token, should be in form IP:PORT".into())
        }
    }
}

impl Display for ListenOn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

impl<'de> Deserialize<'de> for ListenOn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ListenOnVisitor;
        impl<'de> Visitor<'de> for ListenOnVisitor {
            type Value = ListenOn;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an IP address (or `0.0.0.0` or `*`) and port separated by colon, like `1.2.3.4:8080`")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                ListenOn::from_str(v).map_err(|e| E::custom(e))
            }
        }

        deserializer.deserialize_string(ListenOnVisitor)
    }
}

#[derive(Debug, Clone)]
struct HeaderTransform {
    action: HeaderTransformActon,
    value: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
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
                    Err(de::Error::missing_field(
                        "action should be one of add/drop/replace",
                    ))
                }
            }
        }

        const FIELDS: &[&str] = &["add", "drop", "replace", "value"];
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

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct TargetConfig {
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

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
struct ResponseConfig {
    strategy: ResponseStrategy,
    target_selector: ResponseTargetSelector,
    failure_status_regex: String,
    failure_on_timeout: bool,
    timeout_status: ResponseStatus,
    cancel_unneeded_targets: bool,
    #[serde(rename = "override")]
    override_config: Option<OverrideConfig>,
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            strategy: Default::default(),
            target_selector: Default::default(),
            failure_status_regex: "4\\d{2}|5\\d{2}".into(),
            failure_on_timeout: true,
            timeout_status: "504 Gateway Timeout".into(),
            cancel_unneeded_targets: false,
            override_config: None,
        }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
enum ResponseTargetSelector {
    #[default]
    Fastest,
    Slowest,
    Random,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct OverrideConfig {
    status: Option<ResponseStatus>,
    body: Option<String>,
    headers: Option<Vec<HeaderTransform>>,
}

#[derive(Debug, Clone)]
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
    fn from_str(v: &str) -> Result<Self, HttpSplitterError> {
        let re = Regex::new(r"^(?P<code>\d{3})\s*(?P<msg>.*)$")
            .unwrap_or_else(|e| panic!("looks like a BUG: {e}"));
        let caps = re.captures(v);

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

impl AppConfig {
    pub fn from(filename: &String) -> Result<AppConfig, HttpSplitterError> {
        let config = read_to_string(filename).map_err(|e| HttpSplitterError::LoadConfigFile {
            filename: filename.clone(),
            cause: e,
        })?;
        let config: AppConfig = Figment::new().merge(Yaml::string(&config)).extract()?;

        debug!("Application config: {:#?}", config);
        Ok(config)
    }
}
