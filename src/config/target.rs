use hyper::{body::Bytes, http::request::Parts, Uri};
use jaq_interpret::{Ctx, Filter, FilterT, ParseCtx, RcIter, Val};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{json, value::Value as JsonValue, Value};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tracing::{debug, error};

use crate::{context::Context, errors::HttpDragonflyError};

use super::{headers::HeaderTransform, response::ResponseStatus, ConfigValidator};

const DEFAULT_TARGET_TIMEOUT_SEC: u64 = 60;

pub type TargetConfigList = Vec<TargetConfig>;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    id: Option<String>,
    url: String,
    headers: Option<Vec<HeaderTransform>>,
    body: Option<String>,
    #[serde(
        with = "humantime_serde",
        default = "TargetConfig::default_target_timeout"
    )]
    timeout: Duration,
    #[serde(default)]
    on_error: TargetOnErrorAction,
    error_status: Option<ResponseStatus>,
    condition: Option<TargetConditionConfig>,
}

impl TargetConfig {
    fn default_target_timeout() -> Duration {
        Duration::from_secs(DEFAULT_TARGET_TIMEOUT_SEC)
    }

    fn uri(&self) -> Result<Uri, HttpDragonflyError> {
        self.url
            .parse()
            .map_err(|e| HttpDragonflyError::ValidateConfig {
                cause: format!("invalid url `{}`: {e}", self.url),
            })
    }

    pub fn id(&self) -> String {
        if let Some(id) = &self.id {
            id.clone()
        } else {
            format!("TARGET-{}", self.url)
        }
    }

    pub fn host(&self) -> String {
        if let Ok(uri) = self.uri() {
            uri.host().unwrap_or("").to_lowercase()
        } else {
            String::new()
        }
    }

    pub fn url(&self) -> &str {
        self.url.as_ref()
    }

    pub fn headers(&self) -> &Option<Vec<HeaderTransform>> {
        &self.headers
    }

    pub fn body(&self) -> &Option<String> {
        &self.body
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn on_error(&self) -> &TargetOnErrorAction {
        &self.on_error
    }

    pub fn error_status(&self) -> Option<u16> {
        self.error_status
    }

    pub fn condition(&self) -> &Option<TargetConditionConfig> {
        &self.condition
    }
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum TargetOnErrorAction {
    #[default]
    Propagate,
    Status,
    Drop,
}

#[derive(Debug)]
pub enum TargetConditionConfig {
    Default,
    Filter(ConditionFilter),
}

impl TargetConditionConfig {
    fn from_str(value: &str) -> Result<Self, HttpDragonflyError> {
        match value {
            "default" => Ok(TargetConditionConfig::Default),
            _ => Ok(TargetConditionConfig::Filter(ConditionFilter::from_str(
                value,
            )?)),
        }
    }
}

impl From<&str> for TargetConditionConfig {
    fn from(value: &str) -> Self {
        Self::from_str(value).expect("unable to parse conditional expression")
    }
}

impl<'de> Deserialize<'de> for TargetConditionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TargetConditionConfigVisitor;
        impl<'de> Visitor<'de> for TargetConditionConfigVisitor {
            type Value = TargetConditionConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "conditional expression in JQ-like style that returns false/true value",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                TargetConditionConfig::from_str(v).map_err(|e| E::custom(e))
            }
        }

        deserializer.deserialize_string(TargetConditionConfigVisitor)
    }
}

#[derive(Debug)]
pub struct ConditionFilter {
    filter: Filter,
}

impl From<&str> for ConditionFilter {
    fn from(value: &str) -> Self {
        Self::from_str(value).expect("unable to parse conditional expression")
    }
}
impl ConditionFilter {
    fn run(&self, input: JsonValue) -> bool {
        debug!("input=`{:#?}`", input);
        let inputs = RcIter::new(core::iter::empty());
        let out = self.filter.run((Ctx::new([], &inputs), Val::from(input)));

        let out: Vec<String> = out
            .map(|v| format!("{}", v.unwrap_or(Val::Bool(false))))
            .collect();

        let result = out.len() == 1 && out[0] == "true";
        debug!("result=`{result}`");

        result
    }

    fn from_str(value: &str) -> Result<Self, HttpDragonflyError> {
        debug!("filter=`{value}`");
        let mut defs = ParseCtx::new(Vec::new());
        let (f, errs) = jaq_parse::parse(value, jaq_parse::main());
        if !errs.is_empty() {
            errs.iter()
                .for_each(|e| error!("unable to parse conditional expression: {e}"));
            return Err(HttpDragonflyError::ValidateConfig {
                cause: errs[0].to_string(),
            });
        }
        if let Some(f) = f {
            let filter = defs.compile(f);
            Ok(ConditionFilter { filter })
        } else {
            Err(HttpDragonflyError::ValidateConfig {
                cause: "invalid conditional expression".into(),
            })
        }
    }
}

impl ConfigValidator for TargetConfig {
    fn validate(&self) -> Result<(), HttpDragonflyError> {
        // Validate URIs
        self.uri()?;

        // Validate target's error response override
        match self.on_error() {
            TargetOnErrorAction::Propagate | TargetOnErrorAction::Drop => {
                if self.error_status().is_some() {
                    return Err(HttpDragonflyError::ValidateConfig {
                        cause: format!(
                            "`error_status` can be set if `on_error` is `status` only, target `{}`",
                            self.id()
                        ),
                    });
                }
            }
            TargetOnErrorAction::Status => {
                if self.error_status().is_none() {
                    return Err(HttpDragonflyError::ValidateConfig {
                        cause: format!(
                            "`error_status` should be set if `on_error` is `status`, target `{}`",
                            self.id()
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

impl ConfigValidator for [TargetConfig] {
    fn validate(&self) -> Result<(), HttpDragonflyError> {
        // Targets list shouldn't be empty
        if self.is_empty() {
            return Err(HttpDragonflyError::ValidateConfig {
                cause: "at least one target must be configured".into(),
            });
        }

        // Validate each target
        for target in self {
            target.validate()?;
        }

        // Make sure all targets have unique ID
        let unique_targets_count = self
            .iter()
            .map(TargetConfig::id)
            .collect::<HashSet<String>>()
            .len();
        if unique_targets_count != self.len() {
            return Err(HttpDragonflyError::ValidateConfig {
                cause: "all target IDs of the listener should be unique".into(),
            });
        }

        Ok(())
    }
}

pub trait TargetBehavior {
    fn check_condition(&self, ctx: &Context, req: &Parts, body: &Bytes) -> bool;
}

impl TargetBehavior for TargetConfig {
    fn check_condition(&self, ctx: &Context, req: &Parts, body: &Bytes) -> bool {
        // Input content
        // .body
        // .env{}
        // .request.headers{}
        // .request.uri.full
        // .request.uri.host
        // .request.uri.path
        // .request.uri.query
        let body: Value = serde_json::from_slice(body).unwrap_or(json!({}));
        let headers: HashMap<String, String> = req
            .headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect();
        let env = ctx.iter().collect::<HashMap<&String, &String>>();
        let input = json!({
            "body": body,
            "env": env,
            "request": {
                "headers": headers,
                "uri": {
                    "full": req.uri.to_string(),
                    "host": req.uri.host(),
                    "path": req.uri.path(),
                    "query": req.uri.query()
                }
            }
        });

        if let TargetConditionConfig::Filter(filter) = self.condition().as_ref().unwrap() {
            filter.run(input)
        } else {
            false
        }
    }
}

#[cfg(test)]

pub mod test_target {
    use super::*;

    pub fn get_test_target() -> TargetConfig {
        TargetConfig {
            id: Some("TEST-TARGET-ID".into()),
            url: "https://www.google.com/test-path?query=some-query".into(),
            headers: None,
            body: None,
            timeout: Duration::from_secs(DEFAULT_TARGET_TIMEOUT_SEC),
            on_error: TargetOnErrorAction::Propagate,
            error_status: None,
            condition: Some(TargetConditionConfig::Default),
        }
    }
}
