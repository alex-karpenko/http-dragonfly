use clap::Parser;
use regex::Regex;
use tracing::debug;
use tracing_subscriber::{filter::LevelFilter, fmt, EnvFilter};

const DEFAULT_ENV_REGEX: &str = "^HTTP_ENV_[a-zA-Z0-9_]+$";

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct CliConfig {
    /// Enable extreme logging (debug)
    #[arg(short, long)]
    debug: bool,

    /// Enable additional logging (info)
    #[arg(short, long)]
    verbose: bool,

    /// Write logs in JSON format
    #[arg(short, long)]
    json_log: bool,

    /// Path to config file
    #[arg(long, short)]
    config: String,

    /// Allowed environment variables mask (regex)
    #[arg(long, short, default_value_t = DEFAULT_ENV_REGEX.to_string(), value_parser=CliConfig::parse_env_mask)]
    env_mask: String,
}

impl CliConfig {
    pub fn new() -> CliConfig {
        let config: CliConfig = Parser::parse();
        config.setup_logger();

        debug!("CLI config: {:#?}", config);

        config
    }

    fn setup_logger(&self) {
        let level_filter = if self.debug {
            LevelFilter::DEBUG
        } else if self.verbose {
            LevelFilter::INFO
        } else {
            LevelFilter::WARN
        };

        let log_filter = EnvFilter::from_default_env().add_directive(level_filter.into());
        let log_format = fmt::format().with_level(true).with_target(self.debug);

        let subscriber = tracing_subscriber::fmt().with_env_filter(log_filter);
        if self.json_log {
            subscriber
                .event_format(log_format.json().flatten_event(true))
                .init();
        } else {
            subscriber.event_format(log_format.compact()).init();
        };
    }

    fn parse_env_mask(mask: &str) -> Result<String, String> {
        if Regex::new(mask).is_err() {
            Err("invalid environment filter regex".into())
        } else {
            Ok(mask.into())
        }
    }

    pub fn config_path(&self) -> String {
        self.config.to_string()
    }

    pub fn env_mask(&self) -> &str {
        self.env_mask.as_ref()
    }
}
