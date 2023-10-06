mod cli;
mod config;
mod context;
mod errors;
mod listener;

use cli::CliConfig;
use config::AppConfig;
use context::Context;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_config = CliConfig::new();
    let ctx = Context::root();
    let app_config = AppConfig::from(&cli_config.config, ctx)?;

    Ok(())
}
