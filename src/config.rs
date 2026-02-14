pub mod headers;
pub mod listener;
pub mod response;
pub mod target;

use crate::context::Context;
use listener::ListenerConfig;
use serde::Deserialize;
use shellexpand::env_with_context_no_errors;
use std::{
    fs::File,
    io,
    io::{BufReader, Read},
    sync::OnceLock,
};
use tracing::{debug, info};

static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();

#[derive(thiserror::Error)]
pub enum ConfigError {
    #[error("unable to load config: {}", .cause)]
    LoadConfig {
        #[from]
        cause: io::Error,
    },
    #[error("unable to parse config: {}", .cause)]
    ParseConfigFile {
        #[from]
        cause: serde_yaml::Error,
    },
    #[error("invalid config: {}", .cause)]
    ValidateConfig { cause: String },
}

impl std::fmt::Debug for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

pub trait ConfigValidator {
    fn validate(&self) -> Result<(), ConfigError>;
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    listeners: Vec<ListenerConfig>,
}

impl AppConfig {
    pub fn new<'a>(filename: String, ctx: &'a Context<'a>) -> Result<&'a AppConfig, ConfigError> {
        let config = AppConfig::from_file(&filename, ctx)?;
        Ok(APP_CONFIG.get_or_init(|| config))
    }

    fn from_file(filename: &String, ctx: &Context) -> Result<AppConfig, ConfigError> {
        info!("Loading config: {filename}");
        let mut file = File::open(filename)?;
        AppConfig::from_reader(&mut file, ctx)
    }

    fn from_reader(reader: &mut dyn Read, ctx: &Context) -> Result<AppConfig, ConfigError> {
        let mut reader = BufReader::new(reader);
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        let config = env_with_context_no_errors(&buf, |v| ctx.get(&v.into()));
        let config: AppConfig = serde_yaml::from_str(&config)?;

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
    fn validate(&self) -> Result<(), ConfigError> {
        if self.listeners().is_empty() {
            return Err(ConfigError::ValidateConfig {
                cause: String::from("at least one listener must be configured"),
            });
        }

        for listener in self.listeners() {
            listener.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::context::test_context;
    use insta::{assert_debug_snapshot, glob};

    const TEST_CONFIGS_FOLDER: &str = "../tests/configs";

    #[test]
    fn good_config() {
        let ctx = test_context::get_test_ctx();
        glob!(
            TEST_CONFIGS_FOLDER,
            "good/*.yaml",
            |path| assert_debug_snapshot!(AppConfig::from_file(
                &String::from(path.to_str().unwrap()),
                ctx
            ))
        );
    }

    #[test]
    fn wrong_config() {
        let ctx = test_context::get_test_ctx();
        glob!(TEST_CONFIGS_FOLDER, "wrong/*.yaml", |path| {
            assert_debug_snapshot!(AppConfig::from_file(
                &String::from(path.to_str().unwrap()),
                ctx
            ))
        });
    }

    #[test]
    fn errors() {
        assert_debug_snapshot!(ConfigError::LoadConfig {
            cause: io::Error::other("snapshot test cause")
        });
        assert_debug_snapshot!(ConfigError::ValidateConfig {
            cause: "snapshot test cause".to_string()
        });
    }
}
