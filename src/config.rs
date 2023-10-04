mod headers;
mod listener;
mod response;
mod splitter;
mod target;

use figment::{
    providers::{Format, Yaml},
    Figment,
};
use serde::Deserialize;
use std::fs::read_to_string;
use tracing::debug;

use crate::errors::HttpSplitterError;

use self::splitter::SplitterListenerConfig;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    splitters: Vec<SplitterListenerConfig>,
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
