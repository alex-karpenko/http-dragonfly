mod cache;
mod signer;

use crate::config::{listener::ListenerConfig, target::TargetConfig, AppConfig};
use aws_config::{identity::IdentityCache, sts::AssumeRoleProvider, BehaviorVersion, SdkConfig};
use aws_credential_types::provider::ProvideCredentials;
use cache::RefreshingCredentials;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, RwLock},
};
use tokio::sync::OnceCell;
use tracing::{debug, info};

pub(crate) use signer::sign_request;

static SDK_CONFIG: OnceCell<SdkConfig> = OnceCell::const_new();
static CREDENTIALS: LazyLock<RwLock<HashMap<Option<String>, Arc<RefreshingCredentials>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

const STS_SESSION_NAME: &str = "http-dragonfly";

#[derive(thiserror::Error, Debug)]
pub(crate) enum AwsAuthError {
    #[error("failed to resolve AWS credentials for target `{target_id}`: {cause}")]
    Credentials {
        target_id: String,
        cause: aws_credential_types::provider::error::CredentialsError,
    },
    #[error(
        "no AWS region configured for target `{target_id}`: set `aws_sigv4.region` or the AWS SDK default region (AWS_REGION / profile)"
    )]
    NoRegion { target_id: String },
}

fn all_aws_sigv4_configs(
    app_config: &AppConfig,
) -> impl Iterator<Item = &crate::config::aws_sigv4::AwsSigV4Config> {
    app_config
        .listeners()
        .iter()
        .flat_map(ListenerConfig::targets)
        .filter_map(TargetConfig::aws_sigv4)
}

pub(crate) fn is_signing_required(app_config: &AppConfig) -> bool {
    all_aws_sigv4_configs(app_config).next().is_some()
}

/// Loads the AWS SDK default config and warms the shared credentials cache,
/// but only if at least one target actually requires SigV4 signing.
pub(crate) async fn init(app_config: &AppConfig) -> Result<(), AwsAuthError> {
    if !is_signing_required(app_config) {
        debug!("no target requires AWS SigV4 signing, skipping AWS SDK init");
        return Ok(());
    }

    info!("initializing AWS SDK: at least one target requires SigV4 signing");
    let sdk_config = SDK_CONFIG
        .get_or_init(|| async {
            aws_config::defaults(BehaviorVersion::latest())
                .identity_cache(IdentityCache::no_cache())
                .load()
                .await
        })
        .await;

    let has_default = all_aws_sigv4_configs(app_config).any(|c| c.role_arn().is_none());
    let role_arns: HashSet<&str> = all_aws_sigv4_configs(app_config)
        .filter_map(|c| c.role_arn())
        .collect();

    if has_default {
        warm(None, sdk_config).await?;
    }
    for role_arn in role_arns {
        warm(Some(role_arn), sdk_config).await?;
    }

    Ok(())
}

async fn warm(role_arn: Option<&str>, sdk_config: &SdkConfig) -> Result<(), AwsAuthError> {
    let provider = provider_for(role_arn, sdk_config).await;
    provider
        .provide_credentials()
        .await
        .map_err(|cause| AwsAuthError::Credentials {
            target_id: role_arn.unwrap_or("<default>").to_string(),
            cause,
        })?;
    Ok(())
}

async fn provider_for(
    role_arn: Option<&str>,
    sdk_config: &SdkConfig,
) -> Arc<RefreshingCredentials> {
    let key = role_arn.map(str::to_string);

    if let Some(provider) = CREDENTIALS
        .read()
        .expect("unable to lock credentials cache, looks like a BUG")
        .get(&key)
    {
        return provider.clone();
    }

    // Build the provider without holding the lock, since building an
    // AssumeRoleProvider requires an `.await` and the lock guard must not be
    // held across it (it would make the caller's future non-Send).
    let base = sdk_config
        .credentials_provider()
        .expect("AWS SDK always provides a default credentials provider chain, looks like a BUG");

    let provider: Arc<RefreshingCredentials> = match role_arn {
        None => Arc::new(RefreshingCredentials::new(Arc::new(base), "default")),
        Some(role_arn) => {
            let assume_role = AssumeRoleProvider::builder(role_arn)
                .configure(sdk_config)
                .session_name(STS_SESSION_NAME)
                .build_from_provider(base)
                .await;
            Arc::new(RefreshingCredentials::new(
                Arc::new(assume_role),
                role_arn.to_string(),
            ))
        }
    };

    // Double-check: another task may have built and inserted the same key
    // while we were building ours. Whichever inserted first wins; the loser
    // is just a discarded (never-fetched-from) provider, not a wasted STS call.
    CREDENTIALS
        .write()
        .expect("unable to lock credentials cache, looks like a BUG")
        .entry(key)
        .or_insert(provider)
        .clone()
}
