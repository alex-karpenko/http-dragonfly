use std::{env, io};
use thiserror::Error;

#[derive(Error)]
pub enum HttpSplitterError {
    #[error("unable to load config file '{}': {}", .filename, .cause)]
    LoadConfigFile { filename: String, cause: io::Error },
    #[error("unable to parse config: {}", .cause.kind)]
    ParseConfigFile {
        #[from]
        cause: figment::Error,
    },
    #[error("unable to substitute environment variable '{}': {}", .variable, .cause)]
    EnvironmentVariableSubstitution {
        variable: String,
        cause: env::VarError,
    },
}

impl std::fmt::Debug for HttpSplitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}
