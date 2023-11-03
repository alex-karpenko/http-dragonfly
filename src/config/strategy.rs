use std::fmt::Display;

use serde::Deserialize;

use crate::{config::target::TargetConditionConfig, errors::HttpDragonflyError};

use super::target::TargetConfig;

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ResponseStrategy {
    AlwaysOverride,
    AlwaysTargetId,
    OkThenFailed,
    OkThenTargetId,
    OkThenOverride,
    FailedThenOk,
    FailedThenTargetId,
    #[default]
    FailedThenOverride,
    ConditionalRouting,
}

impl Display for ResponseStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ResponseStrategy::AlwaysOverride => "always_override",
                ResponseStrategy::AlwaysTargetId => "always_target_id",
                ResponseStrategy::OkThenFailed => "ok_then_failed",
                ResponseStrategy::OkThenTargetId => "ok_then_target_id",
                ResponseStrategy::OkThenOverride => "ok_then_override",
                ResponseStrategy::FailedThenOk => "failed_then_ok",
                ResponseStrategy::FailedThenTargetId => "failed_then_target_id",
                ResponseStrategy::FailedThenOverride => "failed_then_override",
                ResponseStrategy::ConditionalRouting => "conditional_routing",
            }
        )
    }
}

impl ResponseStrategy {
    pub fn validate(
        &self,
        targets: &[TargetConfig],
        target_selector: &Option<String>,
    ) -> Result<(), HttpDragonflyError> {
        // Validate strategy requirements
        match self {
            ResponseStrategy::ConditionalRouting => {
                // Make sure that all targets have condition defined if strategy is conditional_routing
                if targets.iter().any(|t| t.condition().is_none()) {
                    return Err(HttpDragonflyError::InvalidConfig {
                        cause: format!(
                            "all targets must have condition defined because strategy is `{}`",
                            self
                        ),
                    });
                }
                // Ensure singe default condition is present
                let default_count = targets
                    .iter()
                    .filter(|t| {
                        matches!(
                            t.condition().as_ref().unwrap(),
                            TargetConditionConfig::Default
                        )
                    })
                    .count();
                if default_count > 1 {
                    return Err(HttpDragonflyError::InvalidConfig {
                        cause: "more than one default target is defined but only one is allowed"
                            .into(),
                    });
                }
            }
            ResponseStrategy::AlwaysTargetId
            | ResponseStrategy::FailedThenTargetId
            | ResponseStrategy::OkThenTargetId => {
                // Make sure that target_selector has valid target_id specified if strategy is *_target_id
                let target_ids: Vec<String> = targets.iter().map(TargetConfig::id).collect();
                if let Some(target_id) = target_selector {
                    if !target_ids.contains(target_id) {
                        return Err(HttpDragonflyError::InvalidConfig {
                            cause: format!(
                                "`target_selector` points to unknown target_id `{}`",
                                target_id
                            ),
                        });
                    }
                } else {
                    return Err(HttpDragonflyError::InvalidConfig {
                        cause: format!(
                            "`target_selector` should be specified for strategy `{}`",
                            self
                        ),
                    });
                }
            }
            _ => {}
        };

        Ok(())
    }
}
