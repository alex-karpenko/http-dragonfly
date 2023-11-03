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
use std::{collections::HashSet, fs::read_to_string};
use tracing::{debug, info};

use crate::{context::Context, errors::HttpDragonflyError};

use self::{
    listener::ListenerConfig,
    response::ResponseStrategy,
    target::{TargetConditionConfig, TargetConfig, TargetOnErrorAction},
};

pub trait ConfigValidator {
    fn validate(&self) -> Result<(), HttpDragonflyError>;
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    listeners: Vec<ListenerConfig>,
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
            Ok(_) => {
                static APP_CONFIG: OnceCell<AppConfig> = OnceCell::new();
                Ok(APP_CONFIG.get_or_init(|| config))
            }
            Err(e) => Err(e),
        }
    }

    pub fn listeners(&self) -> &[ListenerConfig] {
        self.listeners.as_ref()
    }
}

impl ConfigValidator for AppConfig {
    fn validate(&self) -> Result<(), HttpDragonflyError> {
        for listener in &self.listeners {
            // Make sure all target IDs are unique
            let ids: HashSet<String> = listener.targets().iter().map(TargetConfig::id).collect();
            if ids.len() != listener.targets().len() {
                return Err(HttpDragonflyError::InvalidConfig {
                    cause: format!(
                        "all target IDs of the listener `{}` should be unique",
                        listener.name()
                    ),
                });
            }

            match listener.response().strategy() {
                ResponseStrategy::ConditionalRouting => {
                    // Make sure that all targets have condition defined if strategy is conditional_routing
                    if listener.targets().iter().any(|t| t.condition().is_none()) {
                        return Err(HttpDragonflyError::InvalidConfig {
                            cause: format!("all targets of the listener `{}` must have condition defined because strategy is `{}`", listener.name(), listener.response().strategy()),
                        });
                    }
                    // Ensure singe default condition is present
                    let default_count = listener
                        .targets()
                        .iter()
                        .filter(|t| {
                            matches!(
                                t.condition().as_ref().unwrap(),
                                TargetConditionConfig::Default
                            )
                        })
                        .count();
                    if default_count > 1 {
                        return Err(HttpDragonflyError::InvalidConfig {
                            cause: format!(
                                "more than one default target is defined of the listener `{}` but only one is allowed",
                                listener.name()
                            ),
                        });
                    }
                }
                ResponseStrategy::AlwaysTargetId
                | ResponseStrategy::FailedThenTargetId
                | ResponseStrategy::OkThenTargetId => {
                    // Make sure that target_selector has valid target_id specified if strategy is *_target_id
                    let target_ids: Vec<String> =
                        listener.targets().iter().map(TargetConfig::id).collect();
                    if let Some(target_id) = &listener.response().target_selector() {
                        if !target_ids.contains(target_id) {
                            return Err(HttpDragonflyError::InvalidConfig {
                                cause: format!("`target_selector` points to unknown target_id `{}` in the listener `{}`", target_id, listener.name()),
                            });
                        }
                    } else {
                        return Err(HttpDragonflyError::InvalidConfig {
                            cause: format!("`target_selector` should be specified for strategy `{}` in the listener `{}`", listener.response().strategy(), listener.name()),
                        });
                    }
                }
                _ => {}
            };

            // Validate URLs
            for url in listener.targets().iter().map(TargetConfig::uri) {
                #[allow(clippy::question_mark)]
                if let Err(e) = url {
                    return Err(e);
                }
            }

            // Validate target's error response override
            for target in listener.targets() {
                match target.on_error() {
                    TargetOnErrorAction::Propagate | TargetOnErrorAction::Drop => {
                        if target.error_status().is_some() {
                            return Err(HttpDragonflyError::InvalidConfig {
                                cause: format!("`error_status` can be set if `on_error` is `status` only, in the listener `{}` and target `{}`", listener.name(), target.id()),
                            });
                        }
                    }
                    TargetOnErrorAction::Status => {
                        if target.error_status().is_none() {
                            return Err(HttpDragonflyError::InvalidConfig {
                            cause: format!("`error_status` should be set if `on_error` is `status`, in the listener `{}` and target `{}`", listener.name(), target.id()),
                        });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
