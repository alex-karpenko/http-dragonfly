pub mod headers;
pub mod listener;
pub mod response;
pub mod strategy;
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

static APP_CONFIG: OnceCell<AppConfig> = OnceCell::new();

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
        let config = AppConfig::owned(filename, ctx)?;
        Ok(APP_CONFIG.get_or_init(|| config))
    }

    fn owned(filename: &String, ctx: &Context) -> Result<AppConfig, HttpDragonflyError> {
        info!("Loading config: {filename}");
        let config = read_to_string(filename).map_err(|e| HttpDragonflyError::LoadConfigFile {
            filename: filename.clone(),
            cause: e,
        })?;
        let config = env_with_context_no_errors(&config, |v| ctx.get(&v.into()));
        let config: AppConfig = Figment::new().merge(Yaml::string(&config)).extract()?;

        debug!("Application config: {:#?}", config);
        match config.validate() {
            Ok(_) => Ok(config),
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
            listener.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::context::test_context;
    use insta::{assert_debug_snapshot, glob};

    use super::*;

    const TEST_CONFIGS_FOLDER: &str = "../tests/configs";

    #[test]
    fn good_config() {
        let ctx = test_context::get_test_ctx();
        glob!(
            TEST_CONFIGS_FOLDER,
            "good/*.yaml",
            |path| assert_debug_snapshot!(AppConfig::owned(
                &String::from(path.to_str().unwrap()),
                &ctx
            ))
        );
    }

    #[test]
    fn wrong_config() {
        let ctx = test_context::get_test_ctx();
        glob!(
            TEST_CONFIGS_FOLDER,
            "wrong/*.yaml",
            |path| insta::with_settings!({filters => vec![(r#"unable to parse config: invalid config: found "(.+)" but expected one of (.+),"#, "unable to parse config: invalid config: found [SOMETHING] but expected one of [EXPECTED JQ STATEMENTS]")]},
                {assert_debug_snapshot!(AppConfig::owned(&String::from(path.to_str().unwrap()),&ctx));})
        );
    }
}
