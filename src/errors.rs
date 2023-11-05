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
    ValidateConfig { cause: String },
}

impl std::fmt::Debug for HttpDragonflyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use figment::Error;
    use insta::assert_debug_snapshot;

    use super::*;

    #[test]
    fn errors() {
        assert_debug_snapshot!(HttpDragonflyError::LoadConfigFile {
            filename: "test-config.yaml".into(),
            cause: io::Error::new(ErrorKind::Other, "snapshot test cause")
        });
        assert_debug_snapshot!(HttpDragonflyError::ParseConfigFile {
            cause: Error::from("snapshot test cause".to_string())
        });
        assert_debug_snapshot!(HttpDragonflyError::ValidateConfig {
            cause: "snapshot test cause".to_string()
        });
    }
}
