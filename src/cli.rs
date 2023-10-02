use clap::Parser;
use tracing::debug;
use tracing_subscriber::{filter::LevelFilter, fmt, EnvFilter};

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct CliConfig {
    /// Enable extreme logging (debug)
    #[clap(short, long)]
    pub debug: bool,

    /// Enable additional logging (info)
    #[clap(short, long)]
    pub verbose: bool,

    /// Write logs in JSON format
    #[clap(short, long)]
    pub json_log: bool,

    /// Path to config file
    #[clap(long, short)]
    pub config: String,
}

impl CliConfig {
    pub fn new() -> CliConfig {
        let config: CliConfig = Parser::parse();
        debug!("CLI config: {:?}", config);

        config.setup_logger();
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
}
