pub mod headers;
pub mod listener;
pub mod response;
pub mod target;

use figment::{
    providers::{Format, Yaml},
    Figment,
};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use shellexpand::env_with_context_no_errors;
use std::fs::read_to_string;
use tracing::{debug, info};

use crate::{context::Context, errors::HttpDragonflyError};

use self::{listener::ListenerConfig, response::ResponseStrategy, target::TargetConfig};

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub listeners: Vec<ListenerConfig>,
}

impl<'a> AppConfig {
    pub fn new(filename: &String, ctx: &Context) -> Result<&'a AppConfig, HttpDragonflyError> {
        info!("Loading config: {filename}");
        let config = read_to_string(filename).map_err(|e| HttpDragonflyError::LoadConfigFile {
            filename: filename.clone(),
            cause: e,
        })?;
        let config = env_with_context_no_errors(&config, |v| ctx.get(&v.into()));
        let config: AppConfig = Figment::new().merge(Yaml::string(&config)).extract()?;

        debug!("Application config: {:#?}", config);
        match config.validate() {
            Ok(config) => {
                static APP_CONFIG: OnceCell<AppConfig> = OnceCell::new();
                Ok(APP_CONFIG.get_or_init(|| config))
            }
            Err(e) => Err(e),
        }
    }

    fn validate(self) -> Result<AppConfig, HttpDragonflyError> {
        for listener in &self.listeners {
            match listener.response.strategy {
                ResponseStrategy::ConditionalRouting => {
                    // Make sure that all targets have condition defined if strategy is conditional_routing
                    if listener.targets.iter().any(|t| t.condition.is_none()) {
                        return Err(HttpDragonflyError::InvalidConfig {
                            cause: format!("all targets of the listener `{}` must have condition defined because strategy is `{}`", listener.get_name(), listener.response.strategy),
                        });
                    }
                }
                ResponseStrategy::AlwaysTargetId
                | ResponseStrategy::FailedThenTargetId
                | ResponseStrategy::OkThenTargetId => {
                    // Make sure that target_selector has valid target_id specified if strategy is *_target_id
                    let target_ids: Vec<String> =
                        listener.targets.iter().map(TargetConfig::get_id).collect();
                    if let Some(target_id) = &listener.response.target_selector {
                        if !target_ids.contains(target_id) {
                            return Err(HttpDragonflyError::InvalidConfig {
                                cause: format!("`target_selector` points to unknown target_id `{}` in the listener `{}`", target_id, listener.get_name()),
                            });
                        }
                    } else {
                        return Err(HttpDragonflyError::InvalidConfig {
                            cause: format!("`target_selector` should be specified for strategy `{}` in the listener `{}`", listener.response.strategy, listener.get_name()),
                        });
                    }
                }
                _ => {}
            };

            // Validate URLs
            for url in listener.targets.iter().map(TargetConfig::get_uri) {
                if let Err(e) = url {
                    return Err(e);
                }
            }
        }
        Ok(self)
    }
}
