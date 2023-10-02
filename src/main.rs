mod cli;
mod config;
mod errors;

use cli::CliConfig;
use config::AppConfig;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_config = CliConfig::new();
    let app_config = AppConfig::from(&cli_config.config)?;

    Ok(())
}
