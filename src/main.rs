mod cli;
mod config;
mod errors;

use cli::CliConfig;
use config::SplitterConfig;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_config = CliConfig::new();
    let splitter_config = SplitterConfig::from(&cli_config.config)?;

    Ok(())
}
