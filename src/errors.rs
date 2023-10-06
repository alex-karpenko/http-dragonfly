use std::io;
use thiserror::Error;

#[derive(Error)]
pub enum HttpDragonflyError {
    #[error("unable to load config file '{}': {}", .filename, .cause)]
    LoadConfigFile { filename: String, cause: io::Error },
    #[error("unable to parse config: {}", .cause.kind)]
    ParseConfigFile {
        #[from]
        cause: figment::Error,
    },
    #[error("invalid config: {}", .cause)]
    InvalidConfig { cause: String },
}

impl std::fmt::Debug for HttpDragonflyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}
