use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer,
};
use std::{
    collections::HashSet,
    fmt::Display,
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
    time::Duration,
};
use strum_macros::EnumString;
use tracing::debug;

use super::{
    headers::HeaderTransform,
    response::{ResponseBehavior, ResponseConfig},
    strategy::ResponseStrategy,
    target::{TargetConfig, TargetConfigList},
    ConfigValidator,
};

const DEFAULT_LISTENER_PORT: u16 = 8080;
const DEFAULT_LISTENER_TIMEOUT_SEC: u64 = 10;
const INVALID_IP_ADDRESS_ERROR: &str = "IP address isn't valid";

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ListenerConfig {
    name: Option<String>,
    #[serde(rename = "on", default)]
    listen_on: ListenOn,
    #[serde(
        with = "humantime_serde",
        default = "ListenerConfig::default_listener_timeout"
    )]
    timeout: Duration,
    #[serde(default)]
    strategy: ResponseStrategy,
    headers: Option<Vec<HeaderTransform>>,
    methods: Option<HashSet<HttpMethod>>,
    targets: TargetConfigList,
    #[serde(default)]
    response: ResponseConfig,
}

impl ListenerConfig {
    fn default_listener_timeout() -> Duration {
        Duration::from_secs(DEFAULT_LISTENER_TIMEOUT_SEC)
    }

    /// Returns the name of this [`ListenerConfig`].
    pub fn name(&self) -> String {
        if let Some(name) = &self.name {
            name.clone()
        } else {
            format!("LISTENER-{}", self.listen_on)
        }
    }

    /// Returns the socket of this [`ListenerConfig`].
    pub fn socket(&self) -> SocketAddr {
        self.listen_on.as_socket()
    }

    /// Verifies if HTTP method is allowed to be used call for this [`ListenerConfig`]
    pub fn is_method_allowed(&self, method: &str) -> bool {
        debug!("allowed: {:?}", self.methods);
        if let Some(methods) = &self.methods {
            debug!("check for: {:?}", method);
            let method = HttpMethod::from_str(method);
            if let Ok(method) = method {
                methods.contains(&method)
            } else {
                false
            }
        } else {
            true
        }
    }

    /// Returns a reference to the timeout of this [`ListenerConfig`].
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the headers of this [`ListenerConfig`].
    pub fn headers(&self) -> Option<&Vec<HeaderTransform>> {
        self.headers.as_ref()
    }

    /// Returns a reference to the targets of this [`ListenerConfig`].
    pub fn targets(&self) -> &[TargetConfig] {
        self.targets.as_ref()
    }

    /// Returns a reference to the response of this [`ListenerConfig`].
    pub fn response(&self) -> &ResponseConfig {
        &self.response
    }
    pub fn strategy(&self) -> &ResponseStrategy {
        &self.strategy
    }

    pub fn on(&self) -> String {
        format!("{}", self.listen_on)
    }
}

#[derive(Deserialize, Debug, EnumString, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields, rename_all = "UPPERCASE")]
#[strum(ascii_case_insensitive)]
enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

#[derive(Debug, PartialEq)]
struct ListenOn {
    ip: Ipv4Addr,
    port: u16,
}

impl ListenOn {
    fn as_socket(&self) -> SocketAddr {
        SocketAddr::new(self.ip.into(), self.port)
    }
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
    fn new(ip: Ipv4Addr, port: u16) -> Result<Self, String> {
        if port > 0 {
            Ok(Self { ip, port })
        } else {
            Err(format!(
                "port `{port}` is invalid, should be between 1 and 65535"
            ))
        }
    }

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

            ListenOn::new(ip, port)
        } else if splitted.len() == 2 {
            let port: u16 = splitted[1]
                .parse()
                .map_err(|e| format!("invalid port value `{}`: {e}", splitted[1]))?;

            let ip = if splitted[0].is_empty() || splitted[0] == "*" {
                Ipv4Addr::new(0, 0, 0, 0)
            } else {
                Self::parse_ip_address(splitted[0])?
            };

            ListenOn::new(ip, port)
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

impl ConfigValidator for ListenerConfig {
    fn validate(&self) -> Result<(), crate::errors::HttpDragonflyError> {
        self.targets().validate()?;
        self.response().validate()?;

        // Validate strategy requirements
        self.strategy()
            .validate(self.targets(), self.response().target_selector())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP_0000: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
    const IP_1234: Ipv4Addr = Ipv4Addr::new(1, 2, 3, 4);

    #[test]
    fn listen_on() {
        assert_eq!(
            ListenOn::default(),
            ListenOn {
                ip: IP_0000,
                port: DEFAULT_LISTENER_PORT
            }
        );

        assert_eq!(
            ListenOn::from_str("1.2.3.4:8888").unwrap(),
            ListenOn {
                ip: IP_1234,
                port: 8888
            }
        );
        assert_eq!(
            ListenOn::from_str("0.0.0.0:8888").unwrap(),
            ListenOn {
                ip: IP_0000,
                port: 8888
            }
        );
        assert_eq!(
            ListenOn::from_str(":8888").unwrap(),
            ListenOn {
                ip: IP_0000,
                port: 8888
            }
        );
        assert_eq!(
            ListenOn::from_str("*:8888").unwrap(),
            ListenOn {
                ip: IP_0000,
                port: 8888
            }
        );
    }

    #[test]
    fn wrong_listen_on() {
        let wrong_str = [
            "",
            ":",
            "1.2.3.4",
            "1.2.3.4:",
            "111.222.333.444:8080",
            "*:0",
            "*:65536",
            "*:123456",
            "*:str",
            "google.com:8080",
        ];

        for wrong_item in wrong_str {
            assert!(
                ListenOn::from_str(wrong_item).is_err(),
                "unexpectedly deserialized `{}`",
                wrong_item
            );
        }
    }
}
