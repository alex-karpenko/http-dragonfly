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

use self::listener::ListenerConfig;

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
        for listener in self.listeners() {
            match listener.validate() {
                Err(e) => return Err(e),
                _ => continue,
            };
        }

        Ok(())
    }
}
