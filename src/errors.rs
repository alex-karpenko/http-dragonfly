use std::io;
use thiserror::Error;

#[derive(Error)]
pub enum HttpDragonflyError {
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

impl std::fmt::Debug for HttpDragonflyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use std::io::ErrorKind;

    use super::*;

    #[test]
    fn errors() {
        assert_debug_snapshot!(HttpDragonflyError::LoadConfig {
            cause: io::Error::new(ErrorKind::Other, "snapshot test cause")
        });
        assert_debug_snapshot!(HttpDragonflyError::ValidateConfig {
            cause: "snapshot test cause".to_string()
        });
    }
}
