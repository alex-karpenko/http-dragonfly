use super::ConfigValidator;
use crate::config::ConfigError;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AwsSigV4Config {
    service: String,
    region: Option<String>,
    role_arn: Option<String>,
}

impl AwsSigV4Config {
    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    pub fn role_arn(&self) -> Option<&str> {
        self.role_arn.as_deref()
    }
}

impl ConfigValidator for AwsSigV4Config {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.service.trim().is_empty() {
            return Err(ConfigError::ValidateConfig {
                cause: "`aws_sigv4.service` must not be empty".into(),
            });
        }
        if let Some(role_arn) = &self.role_arn {
            if !role_arn.starts_with("arn:") || !role_arn.contains(":role/") {
                return Err(ConfigError::ValidateConfig {
                    cause: format!(
                        "`aws_sigv4.role_arn` doesn't look like a valid IAM role ARN: `{role_arn}`"
                    ),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal() {
        let cfg: AwsSigV4Config = serde_yaml_ng::from_str("service: s3").unwrap();
        assert_eq!(cfg.service(), "s3");
        assert_eq!(cfg.region(), None);
        assert_eq!(cfg.role_arn(), None);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn parses_full() {
        let cfg: AwsSigV4Config = serde_yaml_ng::from_str(
            "service: execute-api\nregion: eu-west-1\nrole_arn: arn:aws:iam::123456789012:role/my-role",
        )
        .unwrap();
        assert_eq!(cfg.service(), "execute-api");
        assert_eq!(cfg.region(), Some("eu-west-1"));
        assert_eq!(
            cfg.role_arn(),
            Some("arn:aws:iam::123456789012:role/my-role")
        );
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_empty_service() {
        let cfg: AwsSigV4Config = serde_yaml_ng::from_str("service: \"\"").unwrap();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_bad_role_arn() {
        let cfg: AwsSigV4Config =
            serde_yaml_ng::from_str("service: s3\nrole_arn: not-an-arn").unwrap();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_unknown_field() {
        let result: Result<AwsSigV4Config, _> =
            serde_yaml_ng::from_str("service: s3\naccess_key: AKIA...");
        assert!(result.is_err());
    }
}
