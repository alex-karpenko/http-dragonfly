use http_dragonfly::{cli::CliConfig, context::RootOsEnvironment};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_config = CliConfig::new();
    let env_mask = cli_config.env_mask().to_string();
    let env_provider = RootOsEnvironment::new(&env_mask);

    http_dragonfly::run(cli_config, env_provider).await
}
