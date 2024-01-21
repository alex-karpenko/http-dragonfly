pub mod echo_server;

use echo_server::echo_server;
use futures_util::Future;
use http_dragonfly::{cli::CliConfig, context::RootOsEnvironment};
use std::time::Duration;

pub async fn run_test_with_config(
    config_path: &str,
    echo_port: u16,
    timeout_sec: u64,
    test: impl Future,
) -> Result<(), String> {
    let echo_server = echo_server(echo_port);
    let cli_config = CliConfig::from_config_path(config_path.into());
    let env_provider = RootOsEnvironment::new("^TEST_HTTP_ENV_[A-Z0-9]+$");
    let server = http_dragonfly::run(cli_config, env_provider);
    let timer = tokio::time::sleep(Duration::from_secs(timeout_sec));

    tokio::select! {
        _ = server => Err("http-dragonfly server has been crashed".into()),
        _ = echo_server => Err("echo server has been crashed".into()),
        _ = timer => Err("test has been timed out".into()),
        _result = test => Ok(())
    }
}
