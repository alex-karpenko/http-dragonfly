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
    /// Constructs CLI config
    pub fn new() -> CliConfig {
        let config: CliConfig = Parser::parse();
        config.setup_logger();

        debug!("CLI config: {:#?}", config);

        config
    }

    /// Creates global logger and set requested log level and format
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

    /// Validates mask (regex) to use for context environment variables
    fn parse_env_mask(mask: &str) -> Result<String, String> {
        let mask = mask.trim();
        if mask.is_empty() || mask == "*" {
            Ok(".+".into())
        } else if Regex::new(mask).is_err() {
            Err("invalid environment filter regex".into())
        } else {
            Ok(mask.into())
        }
    }

    /// Getter for config path
    pub fn config_path(&self) -> String {
        self.config.to_string()
    }

    /// Getter for environment variables mask
    pub fn env_mask(&self) -> &str {
        self.env_mask.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_ron_snapshot;

    use super::*;

    #[test]
    fn parse_correct_env_mask() {
        assert_ron_snapshot!(CliConfig::parse_env_mask(DEFAULT_ENV_REGEX));
        assert_ron_snapshot!(CliConfig::parse_env_mask(".*"));
        assert_ron_snapshot!(CliConfig::parse_env_mask(" .* "));
        assert_ron_snapshot!(CliConfig::parse_env_mask("^something_[0-9]{1,32}-.+$"));
    }

    #[test]
    fn parse_empty_env_mask() {
        assert_ron_snapshot!(CliConfig::parse_env_mask(""));
        assert_ron_snapshot!(CliConfig::parse_env_mask("   "));
    }

    #[test]
    fn parse_asterisk_env_mask() {
        assert_ron_snapshot!(CliConfig::parse_env_mask("*"));
        assert_ron_snapshot!(CliConfig::parse_env_mask(" * "));
    }

    #[test]
    fn parse_incorrect_env_mask() {
        assert_ron_snapshot!(CliConfig::parse_env_mask("\\1"));
        assert_ron_snapshot!(CliConfig::parse_env_mask("  \\1   "));
        assert_ron_snapshot!(CliConfig::parse_env_mask("[1"));
        assert_ron_snapshot!(CliConfig::parse_env_mask("**"));
    }
}
