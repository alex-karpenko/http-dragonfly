mod headers;
mod listener;
mod response;
mod target;

use figment::{
    providers::{Format, Yaml},
    Figment,
};
use serde::Deserialize;
use shellexpand::env_with_context_no_errors;
use std::fs::read_to_string;
use tracing::{debug, info};

use crate::{context::Context, errors::HttpDragonflyError};

use self::listener::ListenerConfig;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    listeners: Vec<ListenerConfig>,
}

impl AppConfig {
    pub fn from(filename: &String, ctx: &Context) -> Result<AppConfig, HttpDragonflyError> {
        info!("Loading config: {filename}");
        let config = read_to_string(filename).map_err(|e| HttpDragonflyError::LoadConfigFile {
            filename: filename.clone(),
            cause: e,
        })?;
        let config = env_with_context_no_errors(&config, |v| ctx.get(&v.into()));
        let config: AppConfig = Figment::new().merge(Yaml::string(&config)).extract()?;

        debug!("Application config: {:#?}", config);
        Ok(config)
    }
}
